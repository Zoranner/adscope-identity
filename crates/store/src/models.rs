use adscope_protocol::{User, UserStatus};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthClientType {
    Web,
    Desktop,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthClientRecord {
    pub client_id: String,
    pub name: String,
    pub client_type: OAuthClientType,
    pub client_secret_hash: Option<String>,
    pub redirect_uris: Vec<String>,
    pub allowed_scopes: Vec<String>,
    pub enabled: bool,
}

impl fmt::Debug for OAuthClientRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthClientRecord")
            .field("client_id", &self.client_id)
            .field("name", &self.name)
            .field("client_type", &self.client_type)
            .field(
                "client_secret_hash_present",
                &self.client_secret_hash.is_some(),
            )
            .field("redirect_uris", &self.redirect_uris)
            .field("allowed_scopes", &self.allowed_scopes)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCodeRecord {
    pub code_hash: String,
    pub issuer: String,
    pub client_id: String,
    pub employee_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub nonce: String,
    pub code_challenge: String,
    pub auth_time: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCodeExchange {
    pub code: AuthorizationCodeRecord,
    pub client: Option<OAuthClientRecord>,
    pub user: Option<User>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub mirror_root_dn: String,
    pub quarantine_ou_dn: String,
    pub upn_suffix: String,
    pub employee_id_attribute: String,
    pub managed_group_id_attribute: String,
    pub syncer_key_hash: String,
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

/// 域的工号属性默认值：未指定时使用，默认值只在这里定义。
pub const DEFAULT_EMPLOYEE_ID_ATTRIBUTE: &str = "employeeID";

/// 域的受管组标识属性默认值：未指定时使用，默认值只在这里定义。
pub const DEFAULT_MANAGED_GROUP_ID_ATTRIBUTE: &str = "adminDescription";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOuOutcome {
    Deleted,
    NotFound,
    NotEmpty,
}

/// 未填写的域属性回落到默认值；空白字符串同样按未填写处理。
pub fn attribute_or_default(value: Option<String>, default: &str) -> String {
    match value.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => value,
        _ => default.to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub mirror_root_dn: Option<String>,
    pub quarantine_ou_dn: Option<String>,
    pub upn_suffix: Option<String>,
    pub employee_id_attribute: Option<String>,
    pub managed_group_id_attribute: Option<String>,
    pub syncer_key_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDirectoryPatch {
    pub employee_id: String,
    pub username: String,
    pub display_name: String,
    pub surname: String,
    pub given_name: String,
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub telephone: Option<String>,
    pub organizational_unit_id: String,
    pub status: UserStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserListFilter {
    pub employee_id: Option<String>,
    pub username: Option<String>,
    pub organizational_unit_id: Option<String>,
    pub status: Option<UserStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserContactPatch {
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub telephone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserCredentialInput {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub password_verifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserCreateInput {
    pub directory: UserDirectoryPatch,
    pub credential: UserCredentialInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialRecord {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub password_verifier: String,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialCiphertextEntry {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub status: UserStatus,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialCiphertextBatch {
    pub server_revision: u64,
    pub batch_revision: u64,
    pub credentials: Vec<CredentialCiphertextEntry>,
    pub has_more: bool,
}
