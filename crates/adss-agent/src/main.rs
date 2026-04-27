use adss_agent::{
    AgentCursor, AgentProcessConfig, AgentRuntime, DryRunDirectoryClient, HttpControlPlaneClient,
};
use adss_contract::DomainConfig;

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
    let mut runtime = AgentRuntime::new(
        config.domain_id.clone(),
        config.agent_id.clone(),
        DomainConfig {
            domain_id: config.domain_id,
            mirror_root_dn: "OU=Mirror,DC=example,DC=com".to_string(),
            quarantine_ou_dn: "OU=Quarantine,DC=example,DC=com".to_string(),
            employee_id_attribute: "employeeID".to_string(),
        },
        AgentCursor {
            structure_version: config.initial_structure_version,
            password_task_cursor: config.initial_password_task_cursor,
        },
        control_plane,
        directory,
    );

    runtime.run_once().await?;
    Ok(())
}
