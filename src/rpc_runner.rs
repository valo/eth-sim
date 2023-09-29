use std::{sync::Arc, time::Duration};

use ethers_core::types::{Block, BlockId, Transaction, TxHash};
use ethers_providers::{Http, HttpRateLimitRetryPolicy, Provider, RetryClient, RetryClientBuilder};
use reqwest::Url;
use reth_revm::StateBuilder;
use revm::{
    db::EthersDB,
    primitives::{BlockEnv, EVMError, ResultAndState, U256},
    EVM,
};

use crate::{
    runner::TransactionRunner,
    utils::{configure_tx_env, h256_to_b256},
};

#[derive(Debug, Clone)]
pub struct RpcRunner<'a> {
    pub rpc_url: String,
    pub block: &'a Block<TxHash>,
}

fn create_retrying_provider(url: &str) -> Provider<RetryClient<Http>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let url = Url::parse(url).unwrap();
    let provider = Http::new_with_client(url, client);

    Provider::new(
        RetryClientBuilder::default()
            .initial_backoff(Duration::from_millis(100))
            .timeout_retries(5)
            .rate_limit_retries(100)
            .compute_units_per_second(330)
            .build(provider, Box::new(HttpRateLimitRetryPolicy)),
    )
}

fn fill_block_env(block_env: &mut BlockEnv, block: &Block<TxHash>) {
    block_env.number = U256::from(block.number.unwrap().as_u64());
    block_env.timestamp = block.timestamp.into();
    block_env.coinbase = block.author.unwrap_or_default().into();
    block_env.difficulty = block.difficulty.into();
    block_env.prevrandao = block.mix_hash.map(h256_to_b256);
    block_env.basefee = block.base_fee_per_gas.unwrap_or_default().into();
    block_env.gas_limit = block.gas_limit.into();
}

impl TransactionRunner for RpcRunner<'_> {
    /// Runs the transaction and returns the raw call result.
    fn run(&self, tx: &Transaction) -> Result<ResultAndState, EVMError<String>> {
        let provider = create_retrying_provider(&self.rpc_url);
        let provider = Arc::new(provider);

        let ethersdb = EthersDB::new(
            provider.clone(),
            Some(BlockId::from(self.block.number.unwrap().as_u64())),
        )
        .unwrap();

        let db = StateBuilder::new_with_database(Box::new(ethersdb)).build();

        let mut evm = EVM::new();
        evm.database(db);

        fill_block_env(&mut evm.env.block, &self.block);
        configure_tx_env(&mut evm.env, &tx);

        evm.transact()
            .map_err(|_e| EVMError::Database(String::from("Error running transaction")))
    }
}
