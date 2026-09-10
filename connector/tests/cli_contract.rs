use adscope_connector::{ConnectorCommand, ConnectorInstallConfig, write_install_config};
use std::fs;
use std::path::PathBuf;

#[test]
fn cli_parses_console_service_and_version_commands() {
    assert_eq!(
        ConnectorCommand::parse(["adscope-connector", "--runtime-dir", r"C:\ADSCOPE"]).unwrap(),
        ConnectorCommand::Console {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE")
        }
    );
    assert_eq!(
        ConnectorCommand::parse([
            "adscope-connector",
            "--service",
            "--runtime-dir",
            r"C:\ADSCOPE"
        ])
        .unwrap(),
        ConnectorCommand::Service {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE")
        }
    );
    assert_eq!(
        ConnectorCommand::parse(["adscope-connector", "--version"]).unwrap(),
        ConnectorCommand::Version
    );
    assert_eq!(
        ConnectorCommand::parse([
            "adscope-connector",
            "--configure",
            "--runtime-dir",
            r"C:\ADSCOPE",
            "--center-url",
            "https://center.example.com",
            "--directory-domain-id",
            "domain-a",
            "--connector-access-key",
            "connector-a-key",
            "--domain-controller-ldap-url",
            "ldap://dc01.example.com:389",
            "--match-legacy-users-by-username",
            "1",
        ])
        .unwrap(),
        ConnectorCommand::Configure {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE"),
            config: ConnectorInstallConfig {
                center_url: "https://center.example.com".to_string(),
                directory_domain_id: "domain-a".to_string(),
                connector_access_key: "connector-a-key".to_string(),
                domain_controller_ldap_url: "ldap://dc01.example.com:389".to_string(),
                match_legacy_users_by_username: true,
            },
        }
    );
}

#[test]
fn configure_command_writes_complete_runtime_environment_file() {
    let runtime_dir = std::env::temp_dir().join(format!(
        "adscope-connector-config-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let config = ConnectorInstallConfig {
        center_url: "https://center.example.com".to_string(),
        directory_domain_id: "domain-a".to_string(),
        connector_access_key: "connector-a-key".to_string(),
        domain_controller_ldap_url: "ldap://dc01.example.com:389".to_string(),
        match_legacy_users_by_username: false,
    };

    write_install_config(&runtime_dir, &config).unwrap();

    let environment = fs::read_to_string(runtime_dir.join(".env")).unwrap();
    assert!(environment.contains("CENTER_URL=https://center.example.com"));
    assert!(environment.contains("DIRECTORY_DOMAIN_ID=domain-a"));
    assert!(environment.contains("CONNECTOR_ACCESS_KEY=connector-a-key"));
    assert!(environment.contains("DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389"));
    assert!(environment.contains("MATCH_LEGACY_USERS_BY_USERNAME=0"));

    let updated_config = ConnectorInstallConfig {
        connector_access_key: "updated-connector-key".to_string(),
        ..config
    };
    write_install_config(&runtime_dir, &updated_config).unwrap();
    let updated_environment = fs::read_to_string(runtime_dir.join(".env")).unwrap();
    assert!(updated_environment.contains("CONNECTOR_ACCESS_KEY=updated-connector-key"));
    assert!(!updated_environment.contains("connector-a-key"));

    fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn cli_rejects_missing_values_unknown_arguments_and_conflicts() {
    assert!(
        ConnectorCommand::parse(["adscope-connector", "--runtime-dir"])
            .unwrap_err()
            .to_string()
            .contains("--runtime-dir requires a value")
    );
    assert!(
        ConnectorCommand::parse(["adscope-connector", "--unknown"])
            .unwrap_err()
            .to_string()
            .contains("unknown argument")
    );
    assert!(
        ConnectorCommand::parse(["adscope-connector", "--service", "--version"])
            .unwrap_err()
            .to_string()
            .contains("cannot be combined")
    );
    assert!(
        ConnectorCommand::parse(["adscope-connector", "--configure", "--center-url"])
            .unwrap_err()
            .to_string()
            .contains("--center-url requires a value")
    );
}
