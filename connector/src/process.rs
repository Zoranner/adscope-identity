use crate::{
    ConfiguredDirectoryClient, ConnectorRunSummary, ConnectorRuntime, ControlPlaneClient,
    DirectoryClient, FileLocalStateStore, HttpControlPlaneClient, LocalStateStore, load_env_file,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

use crate::{ConnectorLogger, ConnectorProcessConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingTarget {
    Console,
    File,
}

pub async fn run_configured_connector<F>(
    runtime_dir: impl AsRef<Path>,
    logging_target: LoggingTarget,
    stop: watch::Receiver<bool>,
    on_ready: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let runtime_dir = runtime_dir.as_ref().canonicalize()?;
    if !runtime_dir.is_dir() {
        anyhow::bail!(
            "runtime directory is not a directory: {}",
            runtime_dir.display()
        );
    }
    std::env::set_current_dir(&runtime_dir)?;
    let logger = match logging_target {
        LoggingTarget::Console => ConnectorLogger::console(),
        LoggingTarget::File => ConnectorLogger::file(runtime_dir.join("logs"))?,
    };
    let result = run_with_logger(runtime_dir, stop, on_ready, &logger).await;
    if let Err(error) = &result {
        logger.log_process_error(error);
    }
    result
}

async fn run_with_logger<F>(
    runtime_dir: PathBuf,
    stop: watch::Receiver<bool>,
    on_ready: F,
    logger: &ConnectorLogger,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    load_env_file(runtime_dir.join(".env"))?;
    let config = ConnectorProcessConfig::from_env()?;
    logger.log_startup(&config);

    let control_plane = HttpControlPlaneClient::new(
        config.center_url.clone(),
        config.connector_key.clone(),
        Duration::from_secs(config.http_timeout_seconds),
    )?;
    let directory = ConfiguredDirectoryClient::from_process_config(&config)?;
    let state_path = resolve_runtime_path(&runtime_dir, &config.state_path);
    let local_state = FileLocalStateStore::new(state_path);
    let mut runtime = ConnectorRuntime::new(
        config.domain_id.clone(),
        control_plane,
        directory,
        local_state,
    )
    .with_operation_timeout(Duration::from_secs(config.operation_timeout_seconds));

    if *stop.borrow() {
        return Ok(());
    }
    on_ready()?;

    run_connector_loop(
        &mut runtime,
        Duration::from_secs(config.interval_seconds),
        stop,
        |result| logger.log_run_result(result),
    )
    .await;
    Ok(())
}

pub async fn run_connector_loop<P, D, S, F>(
    runtime: &mut ConnectorRuntime<P, D, S>,
    interval: Duration,
    mut stop: watch::Receiver<bool>,
    mut observe: F,
) where
    P: ControlPlaneClient + Sync,
    D: DirectoryClient + Sync,
    S: LocalStateStore + Sync,
    F: FnMut(&anyhow::Result<ConnectorRunSummary>),
{
    loop {
        let result = runtime.run_once().await;
        observe(&result);

        if *stop.borrow() {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
        }
    }
}

fn resolve_runtime_path(runtime_dir: &Path, configured_path: &str) -> PathBuf {
    let path = Path::new(configured_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        runtime_dir.join(path)
    }
}
