mod cli;
mod config;
mod control_plane;
mod directory;
mod env_file;
mod logging;
mod process;
mod runtime;
mod state;
#[cfg(windows)]
mod windows_service;

pub use cli::ConnectorCommand;
pub use config::{ConnectorProcessConfig, LdapDirectoryConfig};
pub use control_plane::{ControlPlaneClient, HttpControlPlaneClient};
pub use directory::{
    ConfiguredDirectoryClient, DirectoryClient, DirectoryExecutionContext, DirectoryExecutor,
    DryRunDirectoryClient, ExecutionFailure, ExecutionResult, LdapDirectoryClient,
    encode_ad_unicode_password, escape_ldap_dn_value, escape_ldap_filter_value,
    execute_credential_batch, execute_credential_batch_with_timeout, execute_directory_plan,
    execute_directory_plan_with_timeout,
};
pub use env_file::load_env_file;
pub use logging::ConnectorLogger;
pub use process::{LoggingTarget, run_configured_connector, run_connector_loop};
pub use runtime::{ConnectorRunSummary, ConnectorRuntime};
pub use state::{FileLocalStateStore, LocalRevisionState, LocalStateLoad, LocalStateStore};
#[cfg(windows)]
pub use windows_service::{SERVICE_NAME, run_service_dispatcher};
