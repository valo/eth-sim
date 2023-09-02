use anyhow::{Ok, Result};
use bytes::Bytes;
use ethers_contract::BaseContract;
use ethers_providers::{Provider, Http, Middleware};
use foundry_cli::opts::RpcOpts;
use revm::{
    Database,
    db::{CacheDB, EmptyDB, EthersDB},
    primitives::{TransactTo, B160, U256 as rU256},
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

    let latest_block_number = provider.get_block_number().await?;
    let latest_block = provider.get_block(latest_block_number).await?.unwrap();

    // Fetch all the pending transactions
    let mempool = provider.txpool_content().await?;

    println!("Fetched {} pending transactions", mempool.pending.len());

    for (_addr, tx) in mempool.pending {
        for (_nonce, mut tx) in tx {
            let runner = run::TransactionRunner {
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
                    println!("Number of state changes: {}", result.state_changeset.unwrap().len());
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
