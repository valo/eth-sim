use ethers_core::types::Transaction;
use revm::primitives::{ResultAndState, EVMError};

pub trait TransactionRunner {
    fn run(&self, tx: &Transaction) -> Result<ResultAndState, EVMError<String>>;
}