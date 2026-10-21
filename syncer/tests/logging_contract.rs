use adscope_protocol::SyncSummary;
use adscope_syncer::{
    ExecutionFailure, LdapDirectoryConfig, SyncerLogger, SyncerProcessConfig, SyncerRunSummary,
};
use std::fs;

#[test]
fn file_logger_writes_failure_details_without_configuration_secrets() {
    let log_dir = std::env::temp_dir().join(format!(
        "adscope-syncer-logs-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    fs::create_dir_all(&log_dir).unwrap();
    let config = SyncerProcessConfig::new(
        "https://sync.example.com",
        "domain-a",
        "syncer-a-key",
        "adscope-syncer-state.json",
        60,
        LdapDirectoryConfig {
            url: "ldap://dc-a.example.com:389".to_string(),
            server_fqdn: "dc-a.example.com".to_string(),
            adopt_existing_users_by_username: false,
        },
    );
    let summary = SyncerRunSummary {
        directory: SyncSummary {
            failed: 1,
            ..SyncSummary::default()
        },
        directory_failure: Some(ExecutionFailure {
            operation: "ensure_user",
            subject: "1001".to_string(),
            detail: "LDAP permission denied".to_string(),
        }),
        ..SyncerRunSummary::default()
    };

    let logger = SyncerLogger::file(&log_dir).unwrap();
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
    assert!(content.contains("LDAP permission denied"));
    assert!(content.contains("startup config invalid"));
    assert!(!content.contains("connector-a-key"));
    assert!(!content.contains("NewPass123!"));

    fs::remove_dir_all(log_dir).unwrap();
}
