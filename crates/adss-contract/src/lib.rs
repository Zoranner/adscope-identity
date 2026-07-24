use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

const AD_SYSTEM_ATTRIBUTES: &[&str] = &["objectGUID", "objectSid", "distinguishedName"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationalUnit {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub employee_id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub telephone: Option<String>,
    pub organizational_unit_id: String,
    pub status: UserStatus,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub member_employee_ids: Vec<String>,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainDirectoryConfig {
    pub domain_id: String,
    pub mirror_root_dn: String,
    pub quarantine_ou_dn: String,
    pub upn_suffix: String,
    pub employee_id_attribute: String,
    pub managed_group_id_attribute: String,
}

impl DomainDirectoryConfig {
    pub fn example() -> Self {
        Self {
            domain_id: "domain-a".to_string(),
            mirror_root_dn: "OU=Mirror,DC=example,DC=com".to_string(),
            quarantine_ou_dn: "OU=Quarantine,DC=example,DC=com".to_string(),
            upn_suffix: "example.com".to_string(),
            employee_id_attribute: "employeeID".to_string(),
            managed_group_id_attribute: "adminDescription".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryBatch {
    pub server_revision: u64,
    pub batch_revision: u64,
    pub organizational_units: Vec<OrganizationalUnit>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    pub has_more: bool,
}

impl DirectoryBatch {
    pub fn confirm_revision(&self) -> u64 {
        self.batch_revision
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialEntry {
    pub employee_id: String,
    pub plaintext_password: String,
    pub status: UserStatus,
    pub changed_revision: u64,
}

impl fmt::Debug for CredentialEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEntry")
            .field("employee_id", &self.employee_id)
            .field("plaintext_password", &"[redacted]")
            .field("status", &self.status)
            .field("changed_revision", &self.changed_revision)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialBatch {
    pub server_revision: u64,
    pub batch_revision: u64,
    pub credentials: Vec<CredentialEntry>,
    pub has_more: bool,
}

impl CredentialBatch {
    pub fn confirm_revision(&self) -> u64 {
        self.batch_revision
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncChannel {
    Directory,
    Credential,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub succeeded: u32,
    pub failed: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSyncRequest {
    pub domain_id: String,
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
    pub rebuild_directory: bool,
    pub rebuild_credentials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSyncResponse {
    pub directory: DirectoryBatch,
    pub credentials: CredentialBatch,
    pub directory_config: DomainDirectoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfirmRequest {
    pub domain_id: String,
    pub channel: SyncChannel,
    /// Maximum revision fully applied for this batch, not the server's latest revision.
    pub target_revision: u64,
    pub success: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfirmResponse {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserLoginRequest {
    pub employee_id: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserLoginResponse {
    pub employee_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeResponse {
    pub employee_id: String,
    pub credential_revision: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryOperationKind {
    EnsureOu,
    EnsureUser,
    EnsureUserPlacement,
    EnsureGroup,
    EnsureGroupMembers,
    DisableUser,
    MoveUserToQuarantine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryOperation {
    pub kind: DirectoryOperationKind,
    pub subject: String,
    pub target: Option<DirectoryOperationTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DirectoryOperationTarget {
    OrganizationalUnit(OrganizationalUnit),
    User(User),
    UserOrganizationalUnitId(String),
    Group(Group),
    QuarantineDn(String),
    GroupMembers {
        group: Group,
        member_employee_ids: Vec<String>,
    },
}

impl DirectoryOperation {
    pub fn new(kind: DirectoryOperationKind, subject: impl Into<String>) -> Self {
        Self {
            kind,
            subject: subject.into(),
            target: None,
        }
    }

    pub fn with_target(mut self, target: DirectoryOperationTarget) -> Self {
        self.target = Some(target);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryPlan {
    pub target_revision: u64,
    pub operations: Vec<DirectoryOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryPlanError {
    DuplicateOuId(String),
    CyclicOuHierarchy(String),
}

impl fmt::Display for DirectoryPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateOuId(id) => write!(formatter, "duplicate OU ID: {id}"),
            Self::CyclicOuHierarchy(id) => write!(formatter, "cyclic OU hierarchy at: {id}"),
        }
    }
}

impl std::error::Error for DirectoryPlanError {}

impl DirectoryPlan {
    pub fn from_batch(batch: &DirectoryBatch, domain: &DomainDirectoryConfig) -> Self {
        Self::try_from_batch(batch, domain).expect("valid directory batch")
    }

    pub fn try_from_batch(
        batch: &DirectoryBatch,
        domain: &DomainDirectoryConfig,
    ) -> Result<Self, DirectoryPlanError> {
        let mut operations = Vec::new();

        for ou in ordered_organizational_units(&batch.organizational_units)? {
            operations.push(
                DirectoryOperation::new(DirectoryOperationKind::EnsureOu, ou.id.clone())
                    .with_target(DirectoryOperationTarget::OrganizationalUnit(ou.clone())),
            );
        }

        operations.extend(batch.users.iter().map(|user| {
            DirectoryOperation::new(DirectoryOperationKind::EnsureUser, user.employee_id.clone())
                .with_target(DirectoryOperationTarget::User(user.clone()))
        }));
        operations.extend(
            batch
                .users
                .iter()
                .filter(|user| user.status == UserStatus::Active)
                .map(|user| {
                    DirectoryOperation::new(
                        DirectoryOperationKind::EnsureUserPlacement,
                        user.employee_id.clone(),
                    )
                    .with_target(
                        DirectoryOperationTarget::UserOrganizationalUnitId(
                            user.organizational_unit_id.clone(),
                        ),
                    )
                }),
        );
        operations.extend(batch.groups.iter().map(|group| {
            DirectoryOperation::new(DirectoryOperationKind::EnsureGroup, group.id.clone())
                .with_target(DirectoryOperationTarget::Group(group.clone()))
        }));
        operations.extend(batch.groups.iter().map(|group| {
            DirectoryOperation::new(DirectoryOperationKind::EnsureGroupMembers, group.id.clone())
                .with_target(DirectoryOperationTarget::GroupMembers {
                    group: group.clone(),
                    member_employee_ids: group.member_employee_ids.clone(),
                })
        }));

        for user in batch
            .users
            .iter()
            .filter(|user| user.status == UserStatus::Disabled)
        {
            operations.push(DirectoryOperation::new(
                DirectoryOperationKind::DisableUser,
                user.employee_id.clone(),
            ));
            operations.push(
                DirectoryOperation::new(
                    DirectoryOperationKind::MoveUserToQuarantine,
                    user.employee_id.clone(),
                )
                .with_target(DirectoryOperationTarget::QuarantineDn(
                    domain.quarantine_ou_dn.clone(),
                )),
            );
        }

        Ok(Self {
            target_revision: batch.batch_revision,
            operations,
        })
    }
}

fn ordered_organizational_units(
    organizational_units: &[OrganizationalUnit],
) -> Result<Vec<&OrganizationalUnit>, DirectoryPlanError> {
    let mut by_id = BTreeMap::new();

    for ou in organizational_units {
        if by_id.insert(ou.id.as_str(), ou).is_some() {
            return Err(DirectoryPlanError::DuplicateOuId(ou.id.clone()));
        }
    }

    let mut ordered = Vec::new();
    let mut visiting = BTreeMap::new();

    for ou in organizational_units {
        visit_organizational_unit(ou, &by_id, &mut visiting, &mut ordered)?;
    }

    Ok(ordered)
}

fn visit_organizational_unit<'a>(
    ou: &'a OrganizationalUnit,
    by_id: &BTreeMap<&str, &'a OrganizationalUnit>,
    visiting: &mut BTreeMap<&'a str, bool>,
    ordered: &mut Vec<&'a OrganizationalUnit>,
) -> Result<(), DirectoryPlanError> {
    match visiting.get(ou.id.as_str()) {
        Some(true) => return Ok(()),
        Some(false) => return Err(DirectoryPlanError::CyclicOuHierarchy(ou.id.clone())),
        None => {}
    }

    visiting.insert(ou.id.as_str(), false);
    if let Some(parent_id) = &ou.parent_id
        && let Some(parent) = by_id.get(parent_id.as_str())
    {
        visit_organizational_unit(parent, by_id, visiting, ordered)?;
    }
    if matches!(visiting.get(ou.id.as_str()), Some(false)) {
        ordered.push(ou);
        visiting.insert(ou.id.as_str(), true);
    }

    Ok(())
}

pub fn sanitize_user_attributes(
    attributes: BTreeMap<String, String>,
    whitelist: &[&str],
) -> BTreeMap<String, String> {
    attributes
        .into_iter()
        .filter(|(key, _)| whitelist.contains(&key.as_str()))
        .filter(|(key, _)| !AD_SYSTEM_ATTRIBUTES.contains(&key.as_str()))
        .collect()
}
