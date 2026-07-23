pub mod dry_run;
pub mod ldap;

use adss_contract::{
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DirectoryPlan,
    DomainDirectoryConfig, OrganizationalUnit, SyncSummary,
};
use async_trait::async_trait;
use std::collections::BTreeMap;

use crate::config::AgentProcessConfig;

pub use dry_run::DryRunDirectoryClient;
pub use ldap::{
    LdapDirectoryClient, encode_ad_unicode_password, escape_ldap_dn_value, escape_ldap_filter_value,
};

#[async_trait]
pub trait DirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;
    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;
}

pub enum ConfiguredDirectoryClient {
    DryRun(DryRunDirectoryClient),
    Ldap(LdapDirectoryClient),
}

impl ConfiguredDirectoryClient {
    pub fn from_process_config(config: &AgentProcessConfig) -> anyhow::Result<Self> {
        if config.dry_run {
            return Ok(Self::DryRun(DryRunDirectoryClient));
        }

        let ldap = config
            .ldap
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LDAP settings are required without dry-run"))?;
        Ok(Self::Ldap(LdapDirectoryClient::new(ldap)))
    }
}

#[async_trait]
impl DirectoryClient for ConfiguredDirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(client) => client.apply(operation, context).await,
            Self::Ldap(client) => client.apply(operation, context).await,
        }
    }

    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(client) => client.set_password(credential, context).await,
            Self::Ldap(client) => client.set_password(credential, context).await,
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
        let mut by_id = BTreeMap::new();
        for ou in &batch.organizational_units {
            by_id.insert(ou.id.as_str(), ou);
        }

        let mut organizational_unit_dns = BTreeMap::new();
        for ou in &batch.organizational_units {
            build_organizational_unit_dn(ou, &by_id, &mut organizational_unit_dns, domain)?;
        }

        Ok(Self {
            domain: domain.clone(),
            organizational_unit_dns,
        })
    }

    pub fn organizational_unit_dn(&self, organizational_unit_id: &str) -> anyhow::Result<&str> {
        self.organizational_unit_dns
            .get(organizational_unit_id)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing OU DN for {organizational_unit_id}"))
    }
}

fn build_organizational_unit_dn<'a>(
    ou: &'a OrganizationalUnit,
    by_id: &BTreeMap<&str, &'a OrganizationalUnit>,
    organizational_unit_dns: &mut BTreeMap<String, String>,
    domain: &DomainDirectoryConfig,
) -> anyhow::Result<String> {
    if let Some(dn) = organizational_unit_dns.get(&ou.id) {
        return Ok(dn.clone());
    }

    let parent_dn = match &ou.parent_id {
        Some(parent_id) => {
            let parent = by_id
                .get(parent_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing parent OU {parent_id} for {}", ou.id))?;
            build_organizational_unit_dn(parent, by_id, organizational_unit_dns, domain)?
        }
        None => domain.mirror_root_dn.clone(),
    };
    let dn = format!("OU={},{}", escape_ldap_dn_value(&ou.name), parent_dn);
    organizational_unit_dns.insert(ou.id.clone(), dn.clone());
    Ok(dn)
}

pub struct DirectoryExecutor<C> {
    client: C,
}

impl<C> DirectoryExecutor<C> {
    pub fn new(client: C) -> Self {
        Self { client }
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
    ) -> anyhow::Result<SyncSummary> {
        Ok(execute_directory_plan(&self.client, plan, context).await)
    }
}

pub async fn execute_directory_plan<C>(
    client: &C,
    plan: &DirectoryPlan,
    context: &DirectoryExecutionContext,
) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, operation) in plan.operations.iter().enumerate() {
        match client.apply(operation, context).await {
            Ok(()) => summary.succeeded += 1,
            Err(_) => {
                summary.failed += 1;
                summary.skipped += (plan.operations.len() - index - 1) as u32;
                break;
            }
        }
    }

    summary
}

pub async fn execute_credential_batch<C>(
    client: &C,
    batch: &CredentialBatch,
    context: &DirectoryExecutionContext,
) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, credential) in batch.credentials.iter().enumerate() {
        match client.set_password(credential, context).await {
            Ok(()) => summary.succeeded += 1,
            Err(_) => {
                summary.failed += 1;
                summary.skipped += (batch.credentials.len() - index - 1) as u32;
                break;
            }
        }
    }

    summary
}
