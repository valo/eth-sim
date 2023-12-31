use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use dotenv;
use ethers_core::types::Block;
use ethers_core::types::BlockNumber;
use ethers_core::types::Transaction;
use ethers_core::types::H256;
use ethers_providers::Middleware;
use ethers_providers::Provider;
use ethers_providers::StreamExt;
use ethers_providers::Ws;

use reth_runner::RethRunnerBuilder;
use revm::primitives::EVMError;
use revm::primitives::ResultAndState;
use tokio::task::JoinHandle;

use crate::runner::TransactionRunner;
mod reth_runner;
mod rpc_runner;
mod runner;
mod utils;

struct SimulationResult {
    pub elapsed_time: Duration,
    pub result: Result<ResultAndState, EVMError<String>>,
}

fn watch_new_blocks(
    provider: Arc<Provider<Ws>>,
    latest_block: Arc<Mutex<Block<H256>>>,
) -> JoinHandle<eyre::Result<()>> {
    tokio::spawn(async move {
        let mut block_stream = provider.subscribe_blocks().await.unwrap().fuse();

        loop {
            if let Some(block) = block_stream.next().await {
                let mut latest_block = latest_block.lock().unwrap();
                *latest_block = block;
                println!("New block: {:?}", latest_block.number.unwrap());
            }
        }
    })
}

fn run_rpc_simulation(mut tx: Transaction, current_latest_block: Block<H256>) -> SimulationResult {
    // Run in a separate thread because EthersDB is using block_on, which is not
    // compatible with the tokio runtime
    std::thread::spawn(move || {
        let runner: rpc_runner::RpcRunner<'_> = rpc_runner::RpcRunner {
            rpc_url: std::env::var("HTTP_RPC_URL").expect("HTTP_RPC_URL must be set"),
            block: &current_latest_block,
        };

        println!(
            "New pending transaction: {:?}. Simulating against block number {}",
            tx.hash,
            current_latest_block.number.unwrap()
        );
        // Set the gas price to be the max which the transaction is willing to pay
        tx.gas_price = tx.max_fee_per_gas.or(current_latest_block.base_fee_per_gas);

        let now: Instant = Instant::now();
        let result = runner.run(&tx);
        let elapsed_time = now.elapsed();

        SimulationResult {
            elapsed_time,
            result,
        }
    })
    .join()
    .expect("RPC thread panicked!")
}

fn watch_new_transactions(
    provider: Arc<Provider<Ws>>,
    latest_block: Arc<Mutex<Block<H256>>>,
) -> JoinHandle<eyre::Result<()>> {
    tokio::spawn(async move {
        let mut stream = provider
            .subscribe_pending_txs()
            .await
            .unwrap()
            .transactions_unordered(10)
            .fuse();

        let reth_runner = Arc::new(
            RethRunnerBuilder::new()
                .with_db_path(std::env::var("RETH_DB_PATH").expect("RETH_DB_PATH must be set"))
                .build()?,
        );

        loop {
            if let Some(tx) = stream.next().await {
                let Ok(tx) = tx else {
                    continue;
                };

                let current_latest_block: Block<H256> = { latest_block.lock().unwrap().clone() };
                let latest_block_number = current_latest_block.number.unwrap();

                let rpc_simulation_result = run_rpc_simulation(tx.clone(), current_latest_block);

                match rpc_simulation_result.result {
                    Result::Ok(result) => {
                        println!(
                            "Transaction {:?}: used gas {}, success: {}",
                            tx.hash,
                            result.result.gas_used(),
                            result.result.is_success()
                        );
                        println!("Number of state changes: {}", result.state.len());
                        println!(
                            "Elapsed time: {} ms. {:.2} MGas/s",
                            rpc_simulation_result.elapsed_time.as_millis(),
                            result.result.gas_used() as f64
                                / 1000000.0
                                / rpc_simulation_result.elapsed_time.as_secs_f64()
                        );
                    }
                    Result::Err(e) => {
                        println!("Transaction {:?}: error: {:?}", tx.hash, e);
                    }
                }

                let now: Instant = Instant::now();
                let result = reth_runner.run(&tx);
                let reth_elapsed_time = now.elapsed();

                match result {
                    Result::Ok(result) => {
                        println!(
                            "Transaction {:?}: used gas {}, success: {}",
                            tx.hash,
                            result.result.gas_used(),
                            result.result.is_success()
                        );
                        println!("Number of state changes: {}", result.state.len());
                        println!(
                            "Elapsed time: {} ms. {:.2} MGas/s",
                            reth_elapsed_time.as_millis(),
                            result.result.gas_used() as f64
                                / 1000000.0
                                / reth_elapsed_time.as_secs_f64()
                        );
                        let speedup = rpc_simulation_result.elapsed_time.as_secs_f64()
                            / reth_elapsed_time.as_secs_f64();
                        println!("Speedup: {:.2}x", speedup);
                        eprintln!(
                            "{},{:?},{},{:.5},{:.5}",
                            latest_block_number,
                            tx.hash,
                            result.result.gas_used(),
                            rpc_simulation_result.elapsed_time.as_secs_f64(),
                            reth_elapsed_time.as_secs_f64()
                        );
                    }
                    Result::Err(e) => {
                        println!("Reth simulator error: {:?}", e);
                    }
                }
            }
        }
    })
}

async fn run_rpc_simulator() -> eyre::Result<()> {
    let url = std::env::var("WS_RPC_URL").expect("WS_RPC_URL must be set");
    // create ethers client and wrap it in Arc<M>
    let provider = Provider::connect(url.clone()).await?;
    let provider = Arc::new(provider);
    let latest_block: Arc<Mutex<Block<H256>>> = Arc::new(Mutex::new(
        provider.get_block(BlockNumber::Latest).await?.unwrap(),
    ));

    let block_watcher = watch_new_blocks(provider.clone(), latest_block.clone());

    let transaction_watcher = watch_new_transactions(provider.clone(), latest_block);

    let result = tokio::try_join!(block_watcher, transaction_watcher);
    println!("{:?}", result);

    Ok(())
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenv::dotenv().ok();

    run_rpc_simulator().await?;
    // run_reth_simulator()?;
    Ok(())
}
