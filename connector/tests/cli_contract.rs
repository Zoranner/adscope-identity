use adscope_connector::ConnectorCommand;
use std::path::PathBuf;

#[test]
fn cli_parses_console_service_and_version_commands() {
    assert_eq!(
        ConnectorCommand::parse(["adss-connector", "--runtime-dir", r"C:\ADSS"]).unwrap(),
        ConnectorCommand::Console {
            runtime_dir: PathBuf::from(r"C:\ADSS")
        }
    );
    assert_eq!(
        ConnectorCommand::parse(["adss-connector", "--service", "--runtime-dir", r"C:\ADSS"])
            .unwrap(),
        ConnectorCommand::Service {
            runtime_dir: PathBuf::from(r"C:\ADSS")
        }
    );
    assert_eq!(
        ConnectorCommand::parse(["adss-connector", "--version"]).unwrap(),
        ConnectorCommand::Version
    );
}

#[test]
fn cli_rejects_missing_values_unknown_arguments_and_conflicts() {
    assert!(
        ConnectorCommand::parse(["adss-connector", "--runtime-dir"])
            .unwrap_err()
            .to_string()
            .contains("--runtime-dir requires a value")
    );
    assert!(
        ConnectorCommand::parse(["adss-connector", "--unknown"])
            .unwrap_err()
            .to_string()
            .contains("unknown argument")
    );
    assert!(
        ConnectorCommand::parse(["adss-connector", "--service", "--version"])
            .unwrap_err()
            .to_string()
            .contains("cannot be combined")
    );
}
