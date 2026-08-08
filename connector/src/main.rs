use adscope_connector::{ConnectorCommand, LoggingTarget, run_configured_connector};
use tokio::sync::watch;

fn main() -> anyhow::Result<()> {
    match ConnectorCommand::parse(std::env::args_os())? {
        ConnectorCommand::Version => {
            println!("adscope-connector {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ConnectorCommand::Console { runtime_dir } => {
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
                run_configured_connector(runtime_dir, LoggingTarget::Console, stop_rx, || Ok(()))
                    .await
            })
        }
        ConnectorCommand::Service { runtime_dir } => run_service(runtime_dir),
    }
}

#[cfg(windows)]
fn run_service(runtime_dir: std::path::PathBuf) -> anyhow::Result<()> {
    adscope_connector::run_service_dispatcher(runtime_dir)
}

#[cfg(not(windows))]
fn run_service(_runtime_dir: std::path::PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("--service is only supported on Windows")
}
