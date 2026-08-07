pub mod dry_run;
pub mod ldap;

use adss_protocol::{
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DirectoryPlan,
    DomainDirectoryConfig, SyncSummary,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::time::Duration;

use crate::config::ConnectorProcessConfig;

pub use dry_run::{DryRunDirectoryBatch, DryRunDirectoryClient};
pub use ldap::{
    LdapDirectoryBatch, LdapDirectoryClient, encode_ad_unicode_password, escape_ldap_dn_value,
    escape_ldap_filter_value,
};

#[async_trait]
pub trait DirectoryBatchSession {
    async fn apply(
        &mut self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;
    async fn set_password(
        &mut self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;

    async fn close(self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        Ok(())
    }
}

#[async_trait]
pub trait DirectoryClient {
    type Batch: DirectoryBatchSession + Send;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch>;
}

pub enum ConfiguredDirectoryClient {
    DryRun(DryRunDirectoryClient),
    Ldap(LdapDirectoryClient),
}

pub enum ConfiguredDirectoryBatch {
    DryRun(DryRunDirectoryBatch),
    Ldap(LdapDirectoryBatch),
}

impl ConfiguredDirectoryClient {
    pub fn from_process_config(config: &ConnectorProcessConfig) -> anyhow::Result<Self> {
        if config.dry_run {
            return Ok(Self::DryRun(DryRunDirectoryClient));
        }

        let ldap = config
            .ldap
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LDAP settings are required without dry-run"))?;
        Ok(Self::Ldap(LdapDirectoryClient::with_connection_timeout(
            ldap,
            Duration::from_secs(config.operation_timeout_seconds),
        )))
    }
}

#[async_trait]
impl DirectoryClient for ConfiguredDirectoryClient {
    type Batch = ConfiguredDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        match self {
            Self::DryRun(client) => {
                Ok(ConfiguredDirectoryBatch::DryRun(client.open_batch().await?))
            }
            Self::Ldap(client) => Ok(ConfiguredDirectoryBatch::Ldap(client.open_batch().await?)),
        }
    }
}

#[async_trait]
impl DirectoryBatchSession for ConfiguredDirectoryBatch {
    async fn apply(
        &mut self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(batch) => batch.apply(operation, context).await,
            Self::Ldap(batch) => batch.apply(operation, context).await,
        }
    }

    async fn set_password(
        &mut self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(batch) => batch.set_password(credential, context).await,
            Self::Ldap(batch) => batch.set_password(credential, context).await,
        }
    }

    async fn close(self) -> anyhow::Result<()> {
        match self {
            Self::DryRun(batch) => batch.close().await,
            Self::Ldap(batch) => batch.close().await,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryExecutionContext {
    pub domain: DomainDirectoryConfig,
    organizational_unit_dns: BTreeMap<String, String>,
}

impl DirectoryExecutionContext {
    pub fn from_domain(domain: &DomainDirectoryConfig) -> Self {
        Self {
            domain: domain.clone(),
            organizational_unit_dns: BTreeMap::new(),
        }
    }

    pub fn try_from_batch(
        batch: &DirectoryBatch,
        domain: &DomainDirectoryConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            domain: domain.clone(),
            organizational_unit_dns: batch.organizational_unit_dns.clone(),
        })
    }

    pub fn organizational_unit_dn(&self, organizational_unit_id: &str) -> anyhow::Result<&str> {
        self.organizational_unit_dns
            .get(organizational_unit_id)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing OU DN for {organizational_unit_id}"))
    }
}

pub struct DirectoryExecutor<C> {
    client: C,
    operation_timeout: Duration,
}

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

impl<C> DirectoryExecutor<C> {
    pub fn new(client: C) -> Self {
        Self::with_operation_timeout(client, DEFAULT_OPERATION_TIMEOUT)
    }

    pub fn with_operation_timeout(client: C, operation_timeout: Duration) -> Self {
        Self {
            client,
            operation_timeout,
        }
    }
}

impl<C> DirectoryExecutor<C>
where
    C: DirectoryClient + Sync,
{
    pub async fn execute(
        &self,
        plan: &DirectoryPlan,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<ExecutionResult> {
        Ok(
            execute_directory_plan_with_timeout(
                &self.client,
                plan,
                context,
                self.operation_timeout,
            )
            .await,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure {
    pub operation: &'static str,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub summary: SyncSummary,
    pub failure: Option<ExecutionFailure>,
}

pub async fn execute_directory_plan<C>(
    client: &C,
    plan: &DirectoryPlan,
    context: &DirectoryExecutionContext,
) -> ExecutionResult
where
    C: DirectoryClient + Sync,
{
    execute_directory_plan_with_timeout(client, plan, context, DEFAULT_OPERATION_TIMEOUT).await
}

pub async fn execute_directory_plan_with_timeout<C>(
    client: &C,
    plan: &DirectoryPlan,
    context: &DirectoryExecutionContext,
    operation_timeout: Duration,
) -> ExecutionResult
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();
    if plan.operations.is_empty() {
        return ExecutionResult {
            summary,
            failure: None,
        };
    }
    let mut batch = match tokio::time::timeout(operation_timeout, client.open_batch()).await {
        Ok(Ok(batch)) => batch,
        Ok(Err(error)) => return batch_open_failure("open_directory_batch", error),
        Err(_) => return batch_open_timeout("open_directory_batch", operation_timeout),
    };
    let mut failure = None;

    for (index, operation) in plan.operations.iter().enumerate() {
        match tokio::time::timeout(operation_timeout, batch.apply(operation, context)).await {
            Ok(Ok(())) => summary.succeeded += 1,
            Ok(Err(error)) => {
                summary.failed += 1;
                summary.skipped += (plan.operations.len() - index - 1) as u32;
                failure = Some(ExecutionFailure {
                    operation: operation_name(operation.kind),
                    subject: operation.subject.clone(),
                    detail: format!("{error:#}"),
                });
                break;
            }
            Err(_) => {
                summary.failed += 1;
                summary.skipped += (plan.operations.len() - index - 1) as u32;
                failure = Some(ExecutionFailure {
                    operation: operation_name(operation.kind),
                    subject: operation.subject.clone(),
                    detail: format!(
                        "operation timed out after {} seconds",
                        operation_timeout.as_secs_f64()
                    ),
                });
                break;
            }
        }
    }
    if let Err(error) = batch.close().await
        && failure.is_none()
    {
        summary.failed += 1;
        failure = Some(ExecutionFailure {
            operation: "close_directory_batch",
            subject: context.domain.domain_id.clone(),
            detail: format!("{error:#}"),
        });
    }

    ExecutionResult { summary, failure }
}

pub async fn execute_credential_batch<C>(
    client: &C,
    batch: &CredentialBatch,
    context: &DirectoryExecutionContext,
) -> ExecutionResult
where
    C: DirectoryClient + Sync,
{
    execute_credential_batch_with_timeout(client, batch, context, DEFAULT_OPERATION_TIMEOUT).await
}

pub async fn execute_credential_batch_with_timeout<C>(
    client: &C,
    batch: &CredentialBatch,
    context: &DirectoryExecutionContext,
    operation_timeout: Duration,
) -> ExecutionResult
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();
    if batch.credentials.is_empty() {
        return ExecutionResult {
            summary,
            failure: None,
        };
    }
    let mut session = match tokio::time::timeout(operation_timeout, client.open_batch()).await {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => return batch_open_failure("open_credential_batch", error),
        Err(_) => return batch_open_timeout("open_credential_batch", operation_timeout),
    };
    let mut failure = None;

    for (index, credential) in batch.credentials.iter().enumerate() {
        match tokio::time::timeout(operation_timeout, session.set_password(credential, context))
            .await
        {
            Ok(Ok(())) => summary.succeeded += 1,
            Ok(Err(error)) => {
                summary.failed += 1;
                summary.skipped += (batch.credentials.len() - index - 1) as u32;
                failure = Some(ExecutionFailure {
                    operation: "set_password",
                    subject: credential.employee_id.clone(),
                    detail: format!("{error:#}"),
                });
                break;
            }
            Err(_) => {
                summary.failed += 1;
                summary.skipped += (batch.credentials.len() - index - 1) as u32;
                failure = Some(ExecutionFailure {
                    operation: "set_password",
                    subject: credential.employee_id.clone(),
                    detail: format!(
                        "operation timed out after {} seconds",
                        operation_timeout.as_secs_f64()
                    ),
                });
                break;
            }
        }
    }

    if let Err(error) = session.close().await
        && failure.is_none()
    {
        summary.failed += 1;
        failure = Some(ExecutionFailure {
            operation: "close_credential_batch",
            subject: context.domain.domain_id.clone(),
            detail: format!("{error:#}"),
        });
    }

    ExecutionResult { summary, failure }
}

fn batch_open_failure(operation: &'static str, error: anyhow::Error) -> ExecutionResult {
    ExecutionResult {
        summary: SyncSummary {
            failed: 1,
            ..SyncSummary::default()
        },
        failure: Some(ExecutionFailure {
            operation,
            subject: "ldap".to_string(),
            detail: format!("{error:#}"),
        }),
    }
}

fn batch_open_timeout(operation: &'static str, operation_timeout: Duration) -> ExecutionResult {
    batch_open_failure(
        operation,
        anyhow::anyhow!(
            "batch connection timed out after {} seconds",
            operation_timeout.as_secs_f64()
        ),
    )
}

fn operation_name(kind: adss_protocol::DirectoryOperationKind) -> &'static str {
    use adss_protocol::DirectoryOperationKind;

    match kind {
        DirectoryOperationKind::EnsureOu => "ensure_ou",
        DirectoryOperationKind::EnsureUser => "ensure_user",
        DirectoryOperationKind::EnsureUserPlacement => "ensure_user_placement",
        DirectoryOperationKind::EnsureGroup => "ensure_group",
        DirectoryOperationKind::EnsureGroupMembers => "ensure_group_members",
        DirectoryOperationKind::DisableUser => "disable_user",
        DirectoryOperationKind::MoveUserToQuarantine => "move_user_to_quarantine",
    }
}
