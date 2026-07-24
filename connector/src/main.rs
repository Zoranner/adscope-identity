use adss_connector::{
    ConfiguredDirectoryClient, ConnectorProcessConfig, ConnectorRuntime, FileLocalStateStore,
    HttpControlPlaneClient, load_env_file,
};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env_file(".env")?;
    let config = ConnectorProcessConfig::from_env()?;

    let control_plane =
        HttpControlPlaneClient::new(config.center_url.clone(), config.connector_key.clone());
    let directory = ConfiguredDirectoryClient::from_process_config(&config)?;
    let local_state = FileLocalStateStore::new(config.state_path.clone());
    let mut runtime = ConnectorRuntime::new(
        config.domain_id.clone(),
        control_plane,
        directory,
        local_state,
    );
    let interval = Duration::from_secs(config.interval_seconds);

    loop {
        if let Err(error) = runtime.run_once().await {
            eprintln!("connector sync failed: {error:#}");
        }
        sleep(interval).await;
    }
}
