use adss_agent::{AgentProcessConfig, HttpControlPlaneClient};

#[test]
fn http_control_plane_client_builds_agent_api_urls_from_base_url() {
    let client = HttpControlPlaneClient::new("http://127.0.0.1:8080/", "agent-a-key");

    assert_eq!(
        client.endpoint("/api/agent/poll"),
        "http://127.0.0.1:8080/api/agent/poll"
    );
    assert_eq!(client.agent_key(), "agent-a-key");
}

#[test]
fn agent_process_config_uses_explicit_values_and_defaults_to_zero_cursors() {
    let config = AgentProcessConfig::new(
        "http://127.0.0.1:8080",
        "domain-a",
        "agent-a",
        "agent-a-key",
        true,
    );

    assert_eq!(config.server_url, "http://127.0.0.1:8080");
    assert_eq!(config.domain_id, "domain-a");
    assert_eq!(config.agent_id, "agent-a");
    assert_eq!(config.agent_key, "agent-a-key");
    assert!(config.dry_run);
    assert_eq!(config.initial_structure_version, 0);
    assert_eq!(config.initial_password_task_cursor, 0);
}
