use std::{sync::Arc, convert::Infallible, time::Duration};

use ethers_core::types::{Transaction, Block, TxHash, BlockId};
use ethers_providers::{Provider, Http, RetryClientBuilder, HttpRateLimitRetryPolicy, RetryClient};
use foundry_evm::{
    executor::inspector::cheatcodes::util::configure_tx_env,
    utils::h256_to_b256,
};
use reqwest::Url;
use revm::{primitives::{ResultAndState, EVMError, U256}, db::EthersDB, EVM, StateBuilder};

/// CLI arguments for `cast run`.
#[derive(Debug, Clone)]
pub struct TransactionRunner<'a> {
    pub rpc_url: String,

    pub block: &'a Block<TxHash>,
}

fn create_retrying_provider(url: &str) -> Provider<RetryClient<Http>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
    let url = Url::parse(url).unwrap();
    let provider = Http::new_with_client(url, client);

    Provider::new(
        RetryClientBuilder::default()
        .initial_backoff(Duration::from_millis(100))
        .timeout_retries(5)
        .rate_limit_retries(100)
        .compute_units_per_second(330)
        .build(provider, Box::new(HttpRateLimitRetryPolicy))
    )
}

impl TransactionRunner<'_> {
    /// Runs the transaction and returns the raw call result.
    pub fn run(self, tx: &Transaction) -> Result<ResultAndState, EVMError<()>> {
        let provider = create_retrying_provider(&self.rpc_url);
        let provider = Arc::new(provider);

        let ethersdb = EthersDB::new(
            provider.clone(),
            Some(BlockId::from(self.block.number.unwrap().as_u64())))
            .unwrap();
        let db = StateBuilder::<Infallible>::new()
            .with_database(Box::new(ethersdb))
            .build();

        let mut evm = EVM::new();
        evm.database(db);
        
        evm.env.block.number = U256::from(self.block.number.unwrap().as_u64());
        evm.env.block.timestamp = self.block.timestamp.into();
        evm.env.block.coinbase = self.block.author.unwrap_or_default().into();
        evm.env.block.difficulty = self.block.difficulty.into();
        evm.env.block.prevrandao = self.block.mix_hash.map(h256_to_b256);
        evm.env.block.basefee = self.block.base_fee_per_gas.unwrap_or_default().into();
        evm.env.block.gas_limit = self.block.gas_limit.into();

        configure_tx_env(&mut evm.env, &tx);

        evm.transact()
    }
}
