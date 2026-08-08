use adscope_protocol::{
    ConnectorConfirmRequest, ConnectorSyncRequest, DirectoryBatch, DirectoryPlan, SyncChannel,
    SyncSummary,
};
use std::time::Duration;

use crate::{
    ControlPlaneClient, DirectoryClient, DirectoryExecutionContext, ExecutionFailure,
    LocalRevisionState, LocalStateStore, execute_credential_batch_with_timeout,
    execute_directory_plan_with_timeout,
};
pub struct ConnectorRuntime<P, D, S> {
    domain_id: String,
    control_plane: P,
    directory: D,
    local_state: S,
    operation_timeout: Duration,
}

impl<P, D, S> ConnectorRuntime<P, D, S> {
    pub fn new(
        domain_id: impl Into<String>,
        control_plane: P,
        directory: D,
        local_state: S,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            control_plane,
            directory,
            local_state,
            operation_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_operation_timeout(mut self, operation_timeout: Duration) -> Self {
        self.operation_timeout = operation_timeout;
        self
    }

    pub fn control_plane(&self) -> &P {
        &self.control_plane
    }

    pub fn local_state_store(&self) -> &S {
        &self.local_state
    }
}

impl<P, D, S> ConnectorRuntime<P, D, S>
where
    S: LocalStateStore,
{
    pub fn local_state(&self) -> LocalRevisionState {
        self.local_state
            .load()
            .expect("local state should be readable")
    }
}

impl<P, D, S> ConnectorRuntime<P, D, S>
where
    P: ControlPlaneClient + Sync,
    D: DirectoryClient + Sync,
    S: LocalStateStore + Sync,
{
    pub async fn run_once(&mut self) -> anyhow::Result<ConnectorRunSummary> {
        let local_state_load = self.local_state.load_for_sync()?;
        let mut local_state = local_state_load.state;
        let response = self
            .control_plane
            .sync(ConnectorSyncRequest {
                domain_id: self.domain_id.clone(),
                applied_directory_revision: local_state.applied_directory_revision,
                applied_credential_revision: local_state.applied_credential_revision,
                rebuild_directory: local_state_load.rebuild_directory,
                rebuild_credentials: local_state_load.rebuild_credentials,
            })
            .await?;

        let mut run_summary = ConnectorRunSummary::default();
        let mut first_error = None;

        let directory_result = execute_directory_batch(
            &self.directory,
            &response.directory,
            &response.directory_config,
            self.operation_timeout,
        )
        .await;
        run_summary.directory = directory_result.summary;
        run_summary.directory_failure = directory_result.failure;
        if directory_result.succeeded {
            match self
                .confirm_channel(
                    SyncChannel::Directory,
                    response.directory.confirm_revision(),
                    &mut local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        } else if let Some(error_code) = directory_result.error_code {
            match self
                .confirm_failed_channel(
                    SyncChannel::Directory,
                    response.directory.confirm_revision(),
                    error_code,
                    local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        let credential_context = DirectoryExecutionContext::from_domain(&response.directory_config);
        let credential_result = execute_credential_batch_with_timeout(
            &self.directory,
            &response.credentials,
            &credential_context,
            self.operation_timeout,
        )
        .await;
        let credential_summary = credential_result.summary;
        let credential_succeeded =
            !response.credentials.credentials.is_empty() && credential_summary.failed == 0;
        let credential_error_code = if credential_summary.failed > 0 {
            Some("credential_execution_failed")
        } else {
            None
        };
        run_summary.credentials = credential_summary;
        run_summary.credential_failure = credential_result.failure;
        if credential_succeeded {
            match self
                .confirm_channel(
                    SyncChannel::Credential,
                    response.credentials.confirm_revision(),
                    &mut local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        } else if let Some(error_code) = credential_error_code {
            match self
                .confirm_failed_channel(
                    SyncChannel::Credential,
                    response.credentials.confirm_revision(),
                    error_code,
                    local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(run_summary)
        }
    }

    async fn confirm_channel(
        &self,
        channel: SyncChannel,
        revision: u64,
        local_state: &mut LocalRevisionState,
    ) -> anyhow::Result<()> {
        if !should_confirm(channel, revision, *local_state) {
            return Ok(());
        }

        let response = self
            .control_plane
            .confirm(ConnectorConfirmRequest {
                domain_id: self.domain_id.clone(),
                channel,
                target_revision: revision,
                success: true,
                error_code: None,
            })
            .await?;

        if response.accepted {
            match channel {
                SyncChannel::Directory => {
                    local_state.applied_directory_revision = revision;
                }
                SyncChannel::Credential => {
                    local_state.applied_credential_revision = revision;
                }
            }
            self.local_state.save(*local_state)?;
        }

        Ok(())
    }

    async fn confirm_failed_channel(
        &self,
        channel: SyncChannel,
        revision: u64,
        error_code: &'static str,
        local_state: LocalRevisionState,
    ) -> anyhow::Result<()> {
        if !should_confirm(channel, revision, local_state) {
            return Ok(());
        }

        self.control_plane
            .confirm(ConnectorConfirmRequest {
                domain_id: self.domain_id.clone(),
                channel,
                target_revision: revision,
                success: false,
                error_code: Some(error_code.to_string()),
            })
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectorRunSummary {
    pub directory: SyncSummary,
    pub credentials: SyncSummary,
    pub directory_failure: Option<ExecutionFailure>,
    pub credential_failure: Option<ExecutionFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelExecutionResult {
    summary: SyncSummary,
    succeeded: bool,
    error_code: Option<&'static str>,
    failure: Option<ExecutionFailure>,
}

async fn execute_directory_batch<C>(
    client: &C,
    batch: &DirectoryBatch,
    domain: &adscope_protocol::DomainDirectoryConfig,
    operation_timeout: Duration,
) -> ChannelExecutionResult
where
    C: DirectoryClient + Sync,
{
    if batch.organizational_units.is_empty() && batch.users.is_empty() && batch.groups.is_empty() {
        return ChannelExecutionResult {
            summary: SyncSummary::default(),
            succeeded: false,
            error_code: None,
            failure: None,
        };
    }

    let plan = match DirectoryPlan::try_from_batch(batch, domain) {
        Ok(plan) => plan,
        Err(error) => {
            return ChannelExecutionResult {
                summary: SyncSummary {
                    failed: 1,
                    ..SyncSummary::default()
                },
                succeeded: false,
                error_code: Some("directory_plan_failed"),
                failure: Some(ExecutionFailure {
                    operation: "build_directory_plan",
                    subject: domain.domain_id.clone(),
                    detail: error.to_string(),
                }),
            };
        }
    };
    let context = match DirectoryExecutionContext::try_from_batch(batch, domain) {
        Ok(context) => context,
        Err(error) => {
            return ChannelExecutionResult {
                summary: SyncSummary {
                    failed: 1,
                    ..SyncSummary::default()
                },
                succeeded: false,
                error_code: Some("directory_context_failed"),
                failure: Some(ExecutionFailure {
                    operation: "build_directory_context",
                    subject: domain.domain_id.clone(),
                    detail: format!("{error:#}"),
                }),
            };
        }
    };
    let result =
        execute_directory_plan_with_timeout(client, &plan, &context, operation_timeout).await;
    let succeeded = result.summary.failed == 0;
    let error_code = if succeeded {
        None
    } else {
        Some("directory_execution_failed")
    };

    ChannelExecutionResult {
        summary: result.summary,
        succeeded,
        error_code,
        failure: result.failure,
    }
}

fn should_confirm(channel: SyncChannel, revision: u64, local_state: LocalRevisionState) -> bool {
    match channel {
        SyncChannel::Directory => revision > local_state.applied_directory_revision,
        SyncChannel::Credential => revision > local_state.applied_credential_revision,
    }
}
