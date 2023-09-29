use ethers_core::types::Transaction;
use revm::primitives::{EVMError, ResultAndState};

pub trait TransactionRunner {
    fn run(&self, tx: &Transaction) -> Result<ResultAndState, EVMError<String>>;
}
