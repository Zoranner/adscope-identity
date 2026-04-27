use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const AD_SYSTEM_ATTRIBUTES: &[&str] = &["objectGUID", "objectSid", "distinguishedName"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgUnit {
    pub id: String,
    pub relative_dn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub sam_account_name: String,
    pub relative_dn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMembership {
    pub group_id: String,
    pub member_employee_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Disabled,
    DeletedPendingIsolation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub employee_id: String,
    pub sam_account_name: String,
    pub upn: String,
    pub display_name: String,
    pub relative_dn: String,
    pub status: UserStatus,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesiredState {
    pub version: u64,
    pub ous: Vec<OrgUnit>,
    pub groups: Vec<Group>,
    pub users: Vec<User>,
    pub memberships: Vec<GroupMembership>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainConfig {
    pub domain_id: String,
    pub mirror_root_dn: String,
    pub quarantine_ou_dn: String,
    pub employee_id_attribute: String,
}

impl DomainConfig {
    pub fn example() -> Self {
        Self {
            domain_id: "domain-a".to_string(),
            mirror_root_dn: "OU=Mirror,DC=example,DC=com".to_string(),
            quarantine_ou_dn: "OU=Quarantine,DC=example,DC=com".to_string(),
            employee_id_attribute: "employeeID".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdOperationKind {
    EnsureOu,
    EnsureGroup,
    EnsureUser,
    EnsureUserPlacement,
    EnsureGroupMembership,
    DisableUser,
    MoveUserToQuarantine,
    DeleteUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdOperation {
    pub kind: AdOperationKind,
    pub subject: String,
    pub target_dn: Option<String>,
}

impl AdOperation {
    pub fn new(kind: AdOperationKind, subject: impl Into<String>) -> Self {
        Self {
            kind,
            subject: subject.into(),
            target_dn: None,
        }
    }

    pub fn with_target_dn(mut self, target_dn: impl Into<String>) -> Self {
        self.target_dn = Some(target_dn.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcilePlan {
    pub target_version: u64,
    pub operations: Vec<AdOperation>,
}

impl ReconcilePlan {
    pub fn from_desired_state(state: &DesiredState, domain: &DomainConfig) -> Self {
        let mut operations = Vec::new();

        operations.extend(
            state
                .ous
                .iter()
                .map(|ou| AdOperation::new(AdOperationKind::EnsureOu, ou.relative_dn.clone())),
        );
        operations.extend(state.groups.iter().map(|group| {
            AdOperation::new(AdOperationKind::EnsureGroup, group.relative_dn.clone())
        }));
        operations.extend(
            state
                .users
                .iter()
                .filter(|user| user.status != UserStatus::DeletedPendingIsolation)
                .map(|user| {
                    AdOperation::new(AdOperationKind::EnsureUser, user.employee_id.clone())
                }),
        );
        operations.extend(
            state
                .users
                .iter()
                .filter(|user| user.status != UserStatus::DeletedPendingIsolation)
                .map(|user| {
                    AdOperation::new(
                        AdOperationKind::EnsureUserPlacement,
                        user.employee_id.clone(),
                    )
                    .with_target_dn(user.relative_dn.clone())
                }),
        );
        operations.extend(state.memberships.iter().map(|membership| {
            AdOperation::new(
                AdOperationKind::EnsureGroupMembership,
                format!("{}:{}", membership.group_id, membership.member_employee_id),
            )
        }));

        for user in state
            .users
            .iter()
            .filter(|user| user.status == UserStatus::DeletedPendingIsolation)
        {
            operations.push(AdOperation::new(
                AdOperationKind::DisableUser,
                user.employee_id.clone(),
            ));
            operations.push(
                AdOperation::new(
                    AdOperationKind::MoveUserToQuarantine,
                    user.employee_id.clone(),
                )
                .with_target_dn(domain.quarantine_ou_dn.clone()),
            );
        }

        Self {
            target_version: state.version,
            operations,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordTask {
    pub task_id: u64,
    pub domain_id: String,
    pub employee_id: String,
    #[serde(skip_serializing)]
    pub encrypted_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPollRequest {
    pub domain_id: String,
    pub agent_id: String,
    pub last_structure_version: u64,
    pub password_task_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "state", rename_all = "snake_case")]
pub enum PollStructurePayload {
    NoChange,
    Delta(DesiredState),
    Snapshot(DesiredState),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPollResponse {
    pub structure: PollStructurePayload,
    pub password_tasks: Vec<PasswordTask>,
    pub accepted_password_task_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeResponse {
    pub employee_id: String,
    pub created_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub relative_dn: Option<String>,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterAgentRequest {
    pub registration_token: String,
    pub agent_id: String,
    pub domain_id: String,
    pub certificate_subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub domain_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub succeeded: u32,
    pub failed: u32,
    pub skipped: u32,
    pub pending_manual: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectResult {
    pub object_id: String,
    pub operation: AdOperationKind,
    pub success: bool,
    pub error_code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReportRequest {
    pub domain_id: String,
    pub agent_id: String,
    pub applied_structure_version: u64,
    pub applied_password_task_cursor: u64,
    pub summary: SyncSummary,
    pub object_results: Vec<ObjectResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReportResponse {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftReportRequest {
    pub domain_id: String,
    pub agent_id: String,
    pub observed_structure_version: u64,
    pub drifted_objects: Vec<String>,
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
