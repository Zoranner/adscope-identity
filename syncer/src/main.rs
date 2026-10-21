use adscope_syncer::{
    LoggingTarget, SyncerCommand, protect_runtime_config, run_configured_syncer,
    write_install_config,
};
use tokio::sync::watch;

fn main() -> anyhow::Result<()> {
    match SyncerCommand::parse(std::env::args_os())? {
        SyncerCommand::Version => {
            println!("adscope-syncer {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        SyncerCommand::Configure {
            runtime_dir,
            config,
        } => {
            write_install_config(&runtime_dir, &config)?;
            protect_runtime_config(&runtime_dir)?;
            println!("Syncer configuration written to {}", runtime_dir.display());
            Ok(())
        }
        SyncerCommand::Console { runtime_dir } => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                let (stop_tx, stop_rx) = watch::channel(false);
                tokio::spawn(async move {
                    if tokio::signal::ctrl_c().await.is_ok() {
                        let _ = stop_tx.send(true);
                    }
                });
                run_configured_syncer(runtime_dir, LoggingTarget::Console, stop_rx, || Ok(())).await
            })
        }
        SyncerCommand::Service { runtime_dir } => run_service(runtime_dir),
    }
}

#[cfg(windows)]
fn run_service(runtime_dir: std::path::PathBuf) -> anyhow::Result<()> {
    adscope_syncer::run_service_dispatcher(runtime_dir)
}

#[cfg(not(windows))]
fn run_service(_runtime_dir: std::path::PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("--service is only supported on Windows")
}
