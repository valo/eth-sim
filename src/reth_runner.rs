use std::{sync::Arc, path::Path};

use ethers_core::types::Transaction;
use reth_beacon_consensus::BeaconConsensus;
use reth_blockchain_tree::{TreeExternals, BlockchainTreeConfig, ShareableBlockchainTree, BlockchainTree};
use reth_db::{open_db_read_only, database::Database, mdbx::NoWriteMap};
use reth_primitives::{ChainSpecBuilder, BlockNumberOrTag, H160, U256, ChainSpec};
use reth_provider::{providers::BlockchainProvider, ProviderFactory, StateProviderFactory, BlockReaderIdExt, BlockReader, BlockchainTreePendingStateProvider};
use reth_revm::{db::CacheDB, database::StateProviderDatabase, EVM, Factory as ExecutionFactory, primitives::{TxEnv, TransactTo, ResultAndState}, env::fill_block_env};
use reth_interfaces::blockchain_tree::{BlockchainTreeEngine, BlockchainTreeViewer};
use revm::primitives::EVMError;

use crate::runner::TransactionRunner;

pub struct RethRunner<DB, Tree> 
{
    pub spec: Arc<ChainSpec>,
    pub provider: Arc<BlockchainProvider<DB, Tree>>,
}

fn fill_tx_env(tx_env: &mut TxEnv, tx: &Transaction) {
    tx_env.caller = H160::from(tx.from);
    tx_env.gas_limit = tx.gas.as_u64();
    tx_env.gas_price = match tx.max_fee_per_gas {
        Some(max_fee_per_gas) => max_fee_per_gas.into(),
        None => U256::from(1),
    };
    tx_env.gas_priority_fee = match tx.max_priority_fee_per_gas {
        Some(max_priority_fee_per_gas) => Some(max_priority_fee_per_gas.into()),
        None => None,
    };
    tx_env.transact_to = match tx.to {
        Some(to) => TransactTo::Call(to.into()),
        None => TransactTo::create(),
    };
    tx_env.value = tx.value.into();
    tx_env.data = tx.input.0.clone();
    tx_env.nonce = Some(tx.nonce.as_u64());

}

impl<DB, Tree> RethRunner<DB, Tree>
{
    pub fn new(spec: Arc<ChainSpec>, provider: Arc<BlockchainProvider<DB, Tree>>) -> Self {
        Self {
            spec,
            provider,
        }
    }
}

impl<DB, Tree> TransactionRunner for RethRunner<DB, Tree>
where 
    DB: Database,
    Tree: BlockchainTreeViewer + BlockchainTreePendingStateProvider + BlockchainTreeEngine + Send + Sync,
{
    fn run(&self, tx: &Transaction) -> Result<ResultAndState, EVMError<String>> {   
        let latest_block_header = self.provider
            .sealed_header_by_number_or_tag(BlockNumberOrTag::Latest)
            .map_err(|_e| EVMError::Database(String::from("Error fetching latest sealed header")))?
            .unwrap();
 
        let latest_block = self.provider
            .block_by_hash(latest_block_header.hash)
            .map_err(|_e| EVMError::Database(String::from("Error fetching latest block")))?
            .unwrap();

        let latest_state = self.provider
            .state_by_block_hash(latest_block_header.hash)
            .map_err(|_| EVMError::Database(String::from("Error fetching latest state")))?;
        
        let state = Arc::new(StateProviderDatabase::new(latest_state));
        let mut db = CacheDB::new(Arc::clone(&state));
    
        let mut evm = EVM::new();
        evm.database(&mut db);
    
        fill_block_env(&mut evm.env.block, &self.spec, &latest_block, true);
        fill_tx_env(&mut evm.env.tx, tx);
    
        evm.transact()
            .map_err(|_| EVMError::Database(String::from("Error executing transaction")))
    }
}

pub struct RethRunnerBuilder {
    pub db_path: String,
}

impl RethRunnerBuilder {
    pub fn new() -> Self {
        Self {
            db_path: "./".to_string(),
        }
    }

    pub fn with_db_path(&mut self, db_path: String) -> &mut Self {
        self.db_path = db_path;
        self
    }

    pub fn build(&self) -> eyre::Result<RethRunner<Arc<reth_db::mdbx::Env<NoWriteMap>>, ShareableBlockchainTree<Arc<reth_db::mdbx::Env<NoWriteMap>>, Arc<BeaconConsensus>, ExecutionFactory>>> {
        let db_path = std::env::var("RETH_DB_PATH")?;

        let db = Arc::new(open_db_read_only(Path::new(&db_path), None)?);
        let spec = Arc::new(ChainSpecBuilder::mainnet().build());
        let factory = ProviderFactory::new(db.clone(), spec.clone().into());
    
        let provider = Arc::new({
            let consensus = Arc::new(BeaconConsensus::new(spec.clone()));
            let exec_factory = ExecutionFactory::new(spec.clone());
    
            let externals = TreeExternals::new(db.clone(), consensus, exec_factory, spec.clone());
            let tree_config = BlockchainTreeConfig::default();
            let (canon_state_notification_sender, _receiver) =
                tokio::sync::broadcast::channel(tree_config.max_reorg_depth() as usize * 2);
    
            let tree = ShareableBlockchainTree::new(BlockchainTree::new(
                externals,
                canon_state_notification_sender,
                tree_config,
                None,
            )?);
    
            BlockchainProvider::new(factory, tree)?
        });

        Ok(RethRunner::new(spec, provider))
    }
}