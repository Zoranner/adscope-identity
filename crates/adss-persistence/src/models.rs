use adss_contract::UserStatus;
use serde::{Deserialize, Serialize};

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
    pub agent_key_hash: String,
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDirectoryPatch {
    pub employee_id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub telephone: Option<String>,
    pub organizational_unit_id: String,
    pub status: UserStatus,
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
