use adscope_connector::ConnectorCommand;
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
}
