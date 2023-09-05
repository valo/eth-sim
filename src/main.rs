use std::time::Instant;

use anyhow::{Ok, Result};
use ethers_providers::Http;
use ethers_providers::Provider;
use ethers_providers::Middleware;

mod run;

#[tokio::main]
async fn main() -> Result<()> {
    let url = "https://eth.llamarpc.com";
    // create ethers client and wrap it in Arc<M>
    let provider = Provider::<Http>::try_from(url)?;

    // Fetch all the pending transactions
    let mempool = provider.txpool_content().await?;

    println!("Fetched {} pending transactions", mempool.pending.len());

    let latest_block_number = provider.get_block_number().await?;
    let latest_block = provider.get_block(latest_block_number).await?.unwrap();

    println!("Simulating against block {}", latest_block_number);

    for (_addr, tx) in mempool.pending {
        for (_nonce, mut tx) in tx {
            let runner = run::TransactionRunner {
                rpc_url: url.to_string(),
                block: &latest_block
            };
            // Set the gas price to be the max which the transaction is willing to pay
            tx.gas_price = tx.max_fee_per_gas.or(latest_block.base_fee_per_gas);

            let now = Instant::now();

            let result = runner.run(&tx);

            match result {
                Result::Ok(result) => {
                    println!("Transaction {}: used gas {}, success: {}", tx.hash(), result.result.gas_used(), result.result.is_success());
                    println!("Number of state changes: {}", result.state.len());
                    let elapsed_time = now.elapsed();
                    println!("Elapsed time: {} ms. {} gas/ms", elapsed_time.as_millis(), result.result.gas_used() as f64 / elapsed_time.as_millis() as f64);
                }
                Result::Err(e) => {
                    println!("Transaction {}: error: {:?}", tx.hash(), e);
                    println!("{:?}", tx);
                }
            }
        }
    }

    Ok(())
}
