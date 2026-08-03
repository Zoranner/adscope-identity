use adss_connector::{
    ConnectorLogger, ConnectorProcessConfig, ConnectorRunSummary, ExecutionFailure,
    LdapDirectoryConfig,
};
use adss_protocol::SyncSummary;
use std::fs;

#[test]
fn file_logger_writes_failure_details_without_configuration_secrets() {
    let log_dir = std::env::temp_dir().join(format!(
        "adss-connector-logs-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    fs::create_dir_all(&log_dir).unwrap();
    let config = ConnectorProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "connector-a-key",
        "adss-connector-state.json",
        60,
        false,
        Some(LdapDirectoryConfig {
            url: "ldaps://dc-a.example.com:636".to_string(),
            bind_dn: "CN=Svc,DC=example,DC=com".to_string(),
            bind_password: "BindSecret123!".to_string(),
            accept_invalid_certs: false,
            adopt_existing_users_by_username: false,
        }),
    );
    let summary = ConnectorRunSummary {
        directory: SyncSummary {
            failed: 1,
            ..SyncSummary::default()
        },
        directory_failure: Some(ExecutionFailure {
            operation: "ensure_user",
            subject: "1001".to_string(),
            detail: "LDAPS permission denied".to_string(),
        }),
        ..ConnectorRunSummary::default()
    };

    let logger = ConnectorLogger::file(&log_dir).unwrap();
    logger.log_startup(&config);
    logger.log_run_result(&Ok(summary));
    logger.log_process_error(&anyhow::anyhow!("startup config invalid"));
    drop(logger);

    let content = fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(content.contains("ensure_user"));
    assert!(content.contains("1001"));
    assert!(content.contains("LDAPS permission denied"));
    assert!(content.contains("startup config invalid"));
    assert!(!content.contains("connector-a-key"));
    assert!(!content.contains("BindSecret123!"));
    assert!(!content.contains("NewPass123!"));

    fs::remove_dir_all(log_dir).unwrap();
}
