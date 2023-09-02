use ethers_providers::{Middleware, Provider, Http};
use ethers_core::types::H256;
use eyre::{Result, WrapErr};
use foundry_config::{find_project_root_path, Config, ethers_solc::EvmVersion};
use foundry_evm::{
    executor::{inspector::cheatcodes::util::configure_tx_env, opts::EvmOpts, RawCallResult, fork::CreateFork},
    revm::primitives::U256 as rU256,
    trace::TracingExecutor,
    utils::h256_to_b256,
};
use foundry_cli::{opts::RpcOpts};
use revm::primitives::Env;

/// CLI arguments for `cast run`.
#[derive(Debug, Clone)]
pub struct RunArgs {
    /// The transaction hash.
    pub tx_hash: String,

    /// Opens the transaction in the debugger.
    pub debug: bool,

    /// Print out opcode traces.
    pub trace_printer: bool,

    pub rpc: RpcOpts,

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

impl RunArgs {
    /// Executes the transaction by replaying it
    ///
    /// This replays the entire block the transaction was mined in unless `quick` is set to true
    ///
    /// Note: This executes the transaction(s) as is: Cheatcodes are disabled
    pub async fn run(self) -> Result<RawCallResult> {
        let figment =
            Config::figment_with_root(find_project_root_path(None).unwrap());
        let evm_opts = figment.extract::<EvmOpts>()?;

        let provider = Provider::<Http>::try_from(self.rpc.url.as_ref().unwrap())?;

        let tx_hash: H256 = self.tx_hash.parse().wrap_err("invalid tx hash")?;
        let tx = provider
            .get_transaction(tx_hash)
            .await?
            .ok_or_else(|| eyre::eyre!("tx not found: {:?}", tx_hash))?;

        let tx_block_number = tx
            .block_number
            .ok_or_else(|| eyre::eyre!("tx may still be pending: {:?}", tx_hash))?
            .as_u64();

        let fork_block_number = Some(tx_block_number - 1);

        let (mut env, fork) = get_fork_material(
            self.rpc.url.unwrap(),
            fork_block_number,
            evm_opts
        ).await?;

        let mut executor =
            TracingExecutor::new(env.clone(), fork, self.evm_version, self.debug).await;

        env.block.number = rU256::from(tx_block_number);

        let block = provider.get_block_with_txs(tx_block_number).await?;
        if let Some(ref block) = block {
            env.block.timestamp = block.timestamp.into();
            env.block.coinbase = block.author.unwrap_or_default().into();
            env.block.difficulty = block.difficulty.into();
            env.block.prevrandao = block.mix_hash.map(h256_to_b256);
            env.block.basefee = block.base_fee_per_gas.unwrap_or_default().into();
            env.block.gas_limit = block.gas_limit.into();
        }

        // Execute our transaction
        executor.set_trace_printer(self.trace_printer);

        configure_tx_env(&mut env, &tx);

        Ok(executor.commit_tx_with_env(env)?)
    }
}
