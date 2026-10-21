use adscope_syncer::{SyncerCommand, SyncerInstallConfig, write_install_config};
use std::fs;
use std::path::PathBuf;

#[test]
fn cli_parses_console_service_and_version_commands() {
    assert_eq!(
        SyncerCommand::parse(["adscope-syncer", "--runtime-dir", r"C:\ADSCOPE"]).unwrap(),
        SyncerCommand::Console {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE")
        }
    );
    assert_eq!(
        SyncerCommand::parse([
            "adscope-syncer",
            "--service",
            "--runtime-dir",
            r"C:\ADSCOPE"
        ])
        .unwrap(),
        SyncerCommand::Service {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE")
        }
    );
    assert_eq!(
        SyncerCommand::parse(["adscope-syncer", "--version"]).unwrap(),
        SyncerCommand::Version
    );
    assert_eq!(
        SyncerCommand::parse([
            "adscope-syncer",
            "--configure",
            "--runtime-dir",
            r"C:\ADSCOPE",
            "--core-url",
            "https://core.example.com",
            "--directory-domain-id",
            "domain-a",
            "--syncer-access-key",
            "syncer-a-key",
            "--domain-controller-ldap-url",
            "ldap://dc01.example.com:389",
            "--match-legacy-users-by-username",
            "1",
        ])
        .unwrap(),
        SyncerCommand::Configure {
            runtime_dir: PathBuf::from(r"C:\ADSCOPE"),
            config: SyncerInstallConfig {
                core_url: "https://core.example.com".to_string(),
                directory_domain_id: "domain-a".to_string(),
                syncer_access_key: "syncer-a-key".to_string(),
                domain_controller_ldap_url: "ldap://dc01.example.com:389".to_string(),
                match_legacy_users_by_username: true,
            },
        }
    );
}

#[test]
fn configure_command_writes_complete_runtime_environment_file() {
    let runtime_dir = std::env::temp_dir().join(format!(
        "adscope-syncer-config-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let config = SyncerInstallConfig {
        core_url: "https://core.example.com".to_string(),
        directory_domain_id: "domain-a".to_string(),
        syncer_access_key: "syncer-a-key".to_string(),
        domain_controller_ldap_url: "ldap://dc01.example.com:389".to_string(),
        match_legacy_users_by_username: false,
    };

    write_install_config(&runtime_dir, &config).unwrap();

    let environment = fs::read_to_string(runtime_dir.join(".env")).unwrap();
    assert!(environment.contains("CORE_URL=https://core.example.com"));
    assert!(environment.contains("DIRECTORY_DOMAIN_ID=domain-a"));
    assert!(environment.contains("SYNCER_ACCESS_KEY=syncer-a-key"));
    assert!(environment.contains("DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389"));
    assert!(environment.contains("MATCH_LEGACY_USERS_BY_USERNAME=0"));

    let updated_config = SyncerInstallConfig {
        syncer_access_key: "updated-syncer-key".to_string(),
        ..config
    };
    write_install_config(&runtime_dir, &updated_config).unwrap();
    let updated_environment = fs::read_to_string(runtime_dir.join(".env")).unwrap();
    assert!(updated_environment.contains("SYNCER_ACCESS_KEY=updated-syncer-key"));
    assert!(!updated_environment.contains("syncer-a-key"));

    fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn cli_rejects_missing_values_unknown_arguments_and_conflicts() {
    assert!(
        SyncerCommand::parse(["adscope-syncer", "--runtime-dir"])
            .unwrap_err()
            .to_string()
            .contains("--runtime-dir requires a value")
    );
    assert!(
        SyncerCommand::parse(["adscope-syncer", "--unknown"])
            .unwrap_err()
            .to_string()
            .contains("unknown argument")
    );
    assert!(
        SyncerCommand::parse(["adscope-syncer", "--service", "--version"])
            .unwrap_err()
            .to_string()
            .contains("cannot be combined")
    );
    assert!(
        SyncerCommand::parse(["adscope-syncer", "--configure", "--core-url"])
            .unwrap_err()
            .to_string()
            .contains("--core-url requires a value")
    );
}
