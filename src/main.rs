use anyhow::{Ok, Result};
use bytes::Bytes;
use ethers_contract::BaseContract;
use ethers_core::{abi::parse_abi};
use ethers_providers::{Provider, Http, Middleware};
use foundry_cli::opts::RpcOpts;
use revm::{
    Database,
    db::{CacheDB, EmptyDB, EthersDB},
    primitives::{ExecutionResult, Output, TransactTo, B160, U256 as rU256},
    EVM,
};
use std::{str::FromStr, sync::Arc, convert::Infallible, time::Instant};

mod run;

fn create_evm(cache_db : CacheDB<revm::db::EmptyDBTyped<Infallible>>, pool_address: B160, encoded: Bytes) -> Result<EVM<CacheDB<revm::db::EmptyDBTyped<std::convert::Infallible>>>> {
    // initialise an empty (default) EVM
    let mut evm = EVM::new();

    // insert pre-built database from above
    evm.database(cache_db);

    // fill in missing bits of env struct
    // change that to whatever caller you want to be
    evm.env.tx.caller = B160::from_str("0x0000000000000000000000000000000000000000")?;
    // account you want to transact with
    evm.env.tx.transact_to = TransactTo::Call(pool_address);
    // calldata formed via abigen
    evm.env.tx.data = encoded;
    // transaction value in wei
    evm.env.tx.value = rU256::try_from(0)?;

    Ok(evm)
}

#[tokio::main]
async fn main() -> Result<()> {
    let url = "https://eth.llamarpc.com";
    // create ethers client and wrap it in Arc<M>
    let provider = Provider::<Http>::try_from(url)?;
    let provider = Arc::new(provider);

    // ----------------------------------------------------------- //
    //             Storage slots of UniV2Pair contract             //
    // =========================================================== //
    // storage[5] = factory: address                               //
    // storage[6] = token0: address                                //
    // storage[7] = token1: address                                //
    // storage[8] = (res0, res1, ts): (uint112, uint112, uint32)   //
    // storage[9] = price0CumulativeLast: uint256                  //
    // storage[10] = price1CumulativeLast: uint256                 //
    // storage[11] = kLast: uint256                                //
    // =========================================================== //

    // choose slot of storage that you would like to transact with
    let slot = rU256::from(8);

    // // // ETH/USDT pair on Uniswap V2
    let pool_address = B160::from_str("0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852")?;

    // generate abi for the calldata from the human readable interface
    let abi = BaseContract::from(
        parse_abi(&[
            "function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)",
        ])?
    );

    // // // encode abi into Bytes
    let encoded = abi.encode("getReserves", ())?;

    // // initialize new EthersDB
    let mut ethersdb = EthersDB::new(
        Arc::clone(&provider), 
        None
    ).unwrap();

    // query basic properties of an account incl bytecode
    let acc_info = ethersdb.basic(pool_address).unwrap().unwrap();

    // query value of storage slot at account address
    let value = ethersdb.storage(pool_address, slot).unwrap();

    // initialise empty in-memory-db
    let mut cache_db: CacheDB<revm::db::EmptyDBTyped<std::convert::Infallible>> = CacheDB::new(EmptyDB::default());

    // insert basic account info which was generated via Web3DB with the corresponding address
    cache_db.insert_account_info(pool_address, acc_info);

    // insert our pre-loaded storage slot to the corresponding contract key (address) in the DB
    cache_db
        .insert_account_storage(pool_address, slot, value)
        .unwrap();

    let evm = create_evm(cache_db, pool_address, encoded.0)?;

    // execute transaction without writing to the DB
    let ref_tx = evm.transact_ref().unwrap();
    // select ExecutionResult struct
    let result = ref_tx.result;

    // unpack output call enum into raw bytes
    let value = match result {
        ExecutionResult::Success {
            output: Output::Call(value),
            ..
        } => value,
        result => panic!("Execution failed: {result:?}"),
    };

    // decode bytes to reserves + ts via ethers-rs's abi decode
    let (reserve0, reserve1, ts): (u128, u128, u32) = abi.decode_output("getReserves", value)?;

    // Print emulated getReserves() call output
    println!("Reserve0: {:#?}", reserve0);
    println!("Reserve1: {:#?}", reserve1);
    println!("Timestamp: {:#?}", ts);

    let _encoded = abi.encode("getReserves", ())?;

    let latest_block_number = provider.get_block_number().await?;
    let latest_block = provider.get_block(latest_block_number).await?.unwrap();

    // Fetch all the pending transactions
    let mempool = provider.txpool_content().await?;

    println!("Fetched {} pending transactions", mempool.pending.len());

    for (_addr, tx) in mempool.pending {
        for (_nonce, mut tx) in tx {
            let runner = run::TransactionRunner {
                trace_printer: false,
                rpc: RpcOpts { url: Some(url.to_string()), flashbots: false, jwt_secret: None },
                block: &latest_block,
                evm_version: None,
            };
            // Set the gas price to be the max which the transaction is willing to pay
            tx.gas_price = tx.max_fee_per_gas;

            let now = Instant::now();

            let result = runner.run(&tx).await;

            match result {
                Result::Ok(result) => {
                    println!("Transaction {}: used gas {}, revert: {}", tx.hash(), result.gas_used, result.reverted);
                    let elapsed_time = now.elapsed();
                    println!("Elapsed time: {} ms. {} gas/ms", elapsed_time.as_millis(), result.gas_used as f64 / elapsed_time.as_millis() as f64);
                }
                Result::Err(e) => {
                    println!("Transaction {}: error: {}", tx.hash(), e);
                    println!("{:?}", tx);
                }
            }
        }
    }

    Ok(())
}
