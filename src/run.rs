use ethers_core::types::{Transaction, Block, TxHash};
use eyre::Result;
use foundry_cli::opts::RpcOpts;
use foundry_config::{find_project_root_path, Config, ethers_solc::EvmVersion};
use foundry_evm::{
    executor::{inspector::cheatcodes::util::configure_tx_env, opts::EvmOpts, RawCallResult, fork::CreateFork, Backend, ExecutorBuilder},
    revm::primitives::U256 as rU256,
    utils::{h256_to_b256, evm_spec},
};
use revm::primitives::Env;

/// CLI arguments for `cast run`.
#[derive(Debug, Clone)]
pub struct TransactionRunner<'a> {
    pub rpc: RpcOpts,

    pub block: &'a Block<TxHash>,

    /// The evm version to use.
    ///
    /// Overrides the version specified in the config.
    pub evm_version: Option<EvmVersion>,
}

pub async fn get_fork_material(
    fork_url: String,
    fork_block_number: Option<u64>,
    mut evm_opts: EvmOpts,
) -> eyre::Result<(Env, Option<CreateFork>)> {
    evm_opts.fork_url = Some(fork_url.clone());
    evm_opts.fork_block_number = fork_block_number;

    let env = evm_opts.evm_env().await?;

    let fork = Some(CreateFork { url: evm_opts.fork_url.clone().unwrap(), enable_caching: true, env: env.clone(), evm_opts: evm_opts.clone() });

    Ok((env, fork))
}

impl TransactionRunner<'_> {
    /// Runs the transaction and returns the raw call result.
    pub async fn run(self, tx: &Transaction) -> Result<RawCallResult> {
        let figment =
            Config::figment_with_root(find_project_root_path(None).unwrap());
        let evm_opts = figment.extract::<EvmOpts>()?;

        let fork_block_number = Some(self.block.number.unwrap().as_u64() - 1);

        let (mut env, fork) = get_fork_material(
            self.rpc.url.unwrap(),
            fork_block_number,
            evm_opts
        ).await?;

        let db = Backend::spawn(fork).await;
        let mut executor = ExecutorBuilder::new()
                .spec(evm_spec(self.evm_version.unwrap_or_default()))
                .build(env.clone(), db);

        env.block.number = rU256::from(self.block.number.unwrap().as_u64());
        env.block.timestamp = self.block.timestamp.into();
        env.block.coinbase = self.block.author.unwrap_or_default().into();
        env.block.difficulty = self.block.difficulty.into();
        env.block.prevrandao = self.block.mix_hash.map(h256_to_b256);
        env.block.basefee = self.block.base_fee_per_gas.unwrap_or_default().into();
        env.block.gas_limit = self.block.gas_limit.into();

        configure_tx_env(&mut env, &tx);

        Ok(executor.call_raw_with_env(env)?)
    }
}
