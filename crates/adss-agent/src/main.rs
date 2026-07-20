use adss_agent::{
    AgentProcessConfig, AgentRuntime, DryRunDirectoryClient, FileLocalStateStore,
    HttpControlPlaneClient,
};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AgentProcessConfig::from_env()?;
    if !config.dry_run {
        anyhow::bail!(
            "ADSS_AGENT_DRY_RUN=1 is required until a real LDAPS DirectoryClient is configured"
        );
    }

    let control_plane =
        HttpControlPlaneClient::new(config.server_url.clone(), config.agent_key.clone());
    let directory = DryRunDirectoryClient;
    let local_state = FileLocalStateStore::new(config.state_path.clone());
    let mut runtime = AgentRuntime::new(
        config.domain_id.clone(),
        control_plane,
        directory,
        local_state,
    );
    let interval = Duration::from_secs(config.interval_seconds);

    loop {
        if let Err(error) = runtime.run_once().await {
            eprintln!("agent sync failed: {error:#}");
        }
        sleep(interval).await;
    }
}
