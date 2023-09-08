use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Ok, Result};
use dotenv;
use ethers_core::types::Block;
use ethers_core::types::H256;
use ethers_core::types::BlockNumber;
use ethers_providers::Provider;
use ethers_providers::Middleware;
use ethers_providers::StreamExt;
use ethers_providers::Ws;
use tokio::task::JoinHandle;

mod run;

fn watch_new_blocks(provider: Arc<Provider<Ws>>, latest_block: Arc<Mutex<Block<H256>>>) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let mut block_stream = provider.subscribe_blocks()
        .await
        .unwrap()
        .fuse();

        loop {
            if let Some(block) = block_stream.next().await {
                let mut latest_block = latest_block.lock().unwrap();
                *latest_block = block;
                println!("New block: {:?}", latest_block.number.unwrap());
            }
        }
    })
}

fn watch_new_transactions(provider: Arc<Provider<Ws>>, latest_block: Arc<Mutex<Block<H256>>>) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let mut stream = provider.subscribe_pending_txs()
            .await
            .unwrap()
            .transactions_unordered(10)
            .fuse();

        loop {
            if let Some(tx) = stream.next().await {
                if tx.is_err() { continue; }
                let current_latest_block: Block<H256> = { latest_block.lock().unwrap().clone() };

                // Run in a separate thread because EthersDB is using block_on, which is not
                // compatible with the tokio runtime
                let _ = std::thread::spawn(move || {
                    let mut tx = tx.unwrap();
                    println!("New pending transaction: {:?}. Simulating against block number {}", tx.hash, current_latest_block.number.unwrap());

                    let runner: run::TransactionRunner<'_> = run::TransactionRunner {
                        rpc_url: std::env::var("HTTP_RPC_URL").expect("HTTP_RPC_URL must be set"),
                        block: &current_latest_block
                    };
                    // Set the gas price to be the max which the transaction is willing to pay
                    tx.gas_price = tx.max_fee_per_gas.or(current_latest_block.base_fee_per_gas);

                    let now = Instant::now();

                    let result = runner.run(&tx);

                    match result {
                        Result::Ok(result) => {
                            println!("Transaction {:?}: used gas {}, success: {}", tx.hash, result.result.gas_used(), result.result.is_success());
                            println!("Number of state changes: {}", result.state.len());
                            let elapsed_time = now.elapsed();
                            println!("Elapsed time: {} ms. {} gas/ms", elapsed_time.as_millis(), result.result.gas_used() as f64 / elapsed_time.as_millis() as f64);
                        }
                        Result::Err(e) => {
                            println!("Transaction {:?}: error: {:?}", tx.hash, e);
                        }
                    }
                }).join();
            }
        }
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let url = std::env::var("WS_RPC_URL").expect("WS_RPC_URL must be set");
    // create ethers client and wrap it in Arc<M>
    let provider = Provider::connect(url.clone()).await?;
    let provider = Arc::new(provider);
    let latest_block : Arc<Mutex<Block<H256>>> = Arc::new(Mutex::new(provider.get_block(BlockNumber::Latest).await?.unwrap()));

    let block_watcher = watch_new_blocks(provider.clone(), latest_block.clone());

    let transaction_watcher = watch_new_transactions(provider.clone(), latest_block);

    let _ = tokio::join!(block_watcher, transaction_watcher);
    Ok(())
}
