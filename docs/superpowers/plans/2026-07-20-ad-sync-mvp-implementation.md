# AD Sync MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current task/cursor prototype with the confirmed MVP: current database facts, object `changed_revision`, per-domain applied revisions, and independent directory and credential pull/confirm channels.

**Architecture:** `adss-contract` owns the wire and execution models, `adss-persistence` owns the current fact schema and revision transactions, `adss-server` becomes a repository-backed control plane without an in-memory business Store, and `adss-agent` pulls batches, executes idempotently, confirms only after success, and stores only local applied revisions.

**Tech Stack:** Rust 2024 workspace, Axum, SeaORM, SQLite/PostgreSQL-compatible schema, Reqwest, Tokio, CSharpier not applicable.

---

## File Map

- Modify `crates/adss-contract/src/lib.rs`: replace legacy desired-state, task, report, drift, and registration contracts with MVP directory, credential, pull, and confirm contracts.
- Modify `crates/adss-contract/tests/sync_contract.rs`: verify changed-revision filtering inputs, directory execution planning, disabled user handling, and credential redaction.
- Modify `crates/adss-persistence/src/lib.rs`: replace snapshot/task/cursor/drift/token tables with current fact tables, revision metadata, and repository methods.
- Modify `crates/adss-persistence/tests/orm_repository_contract.rs`: verify transactional revision increments, changed-revision queries, domain applied revision updates, and credential storage.
- Modify `crates/adss-server/src/lib.rs`: remove `Store`, registration, password tasks, drift, and old poll/report; add repository-backed write, pull, confirm, login, and password change handlers.
- Modify `crates/adss-server/src/main.rs`: require `ADSS_DATABASE_URL` for the server and initialize the MVP schema.
- Modify `crates/adss-server/tests/api_contract.rs`: replace old API tests with MVP API contract tests.
- Modify `crates/adss-agent/src/lib.rs`: replace one-shot task runtime with independent directory and credential channels, local revision persistence, and batch confirmation behavior.
- Modify `crates/adss-agent/src/main.rs`: load local state path and interval configuration; run the polling loop.
- Modify `crates/adss-agent/tests/*.rs`: replace old runtime/client/execution tests with MVP runtime, HTTP client, and executor tests.
- Modify `docs/api.md`, `docs/database.md`, `docs/sync-protocol.md`, `docs/operations.md`, `docs/security.md`, and `docs/architecture.md`: align public docs with the MVP design after code behavior is in place.

## Task Contract Model

**Files:**
- Modify: `crates/adss-contract/src/lib.rs`
- Modify: `crates/adss-contract/tests/sync_contract.rs`

- [ ] **Step: Write contract tests first**

Replace `crates/adss-contract/tests/sync_contract.rs` with tests covering the new model:

```rust
use adss_contract::{
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryPlan, DomainDirectoryConfig, Group,
    OrganizationalUnit, SyncChannel, User, UserStatus,
};

#[test]
fn directory_plan_orders_current_objects_by_ad_dependencies() {
    let batch = DirectoryBatch {
        target_revision: 7,
        organizational_units: vec![OrganizationalUnit {
            id: "ou-rd".to_string(),
            name: "研发部".to_string(),
            parent_id: None,
            changed_revision: 7,
        }],
        users: vec![User {
            employee_id: "1001".to_string(),
            username: "zhangsan".to_string(),
            display_name: "张三".to_string(),
            email: Some("zhangsan@example.com".to_string()),
            mobile: Some("13800000000".to_string()),
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Active,
            changed_revision: 7,
        }],
        groups: vec![Group {
            id: "dev".to_string(),
            name: "Developers".to_string(),
            member_employee_ids: vec!["1001".to_string()],
            changed_revision: 7,
        }],
        has_more: false,
    };

    let plan = DirectoryPlan::from_batch(&batch, &DomainDirectoryConfig::example());
    let kinds: Vec<_> = plan.operations.iter().map(|operation| operation.kind).collect();

    assert_eq!(
        kinds,
        vec![
            adss_contract::DirectoryOperationKind::EnsureOu,
            adss_contract::DirectoryOperationKind::EnsureUser,
            adss_contract::DirectoryOperationKind::EnsureUserPlacement,
            adss_contract::DirectoryOperationKind::EnsureGroup,
            adss_contract::DirectoryOperationKind::EnsureGroupMembers,
        ]
    );
}

#[test]
fn disabled_users_are_disabled_and_moved_to_quarantine() {
    let batch = DirectoryBatch {
        target_revision: 9,
        organizational_units: Vec::new(),
        users: vec![User {
            employee_id: "1002".to_string(),
            username: "lisi".to_string(),
            display_name: "李四".to_string(),
            email: None,
            mobile: None,
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Disabled,
            changed_revision: 9,
        }],
        groups: Vec::new(),
        has_more: false,
    };

    let plan = DirectoryPlan::from_batch(&batch, &DomainDirectoryConfig::example());
    let kinds: Vec<_> = plan.operations.iter().map(|operation| operation.kind).collect();

    assert_eq!(
        kinds,
        vec![
            adss_contract::DirectoryOperationKind::DisableUser,
            adss_contract::DirectoryOperationKind::MoveUserToQuarantine,
        ]
    );
}

#[test]
fn credential_debug_output_does_not_include_password_payload() {
    let batch = CredentialBatch {
        target_revision: 12,
        credentials: vec![CredentialEntry {
            employee_id: "1001".to_string(),
            plaintext_password: "Secret123!".to_string(),
            changed_revision: 12,
        }],
        has_more: false,
    };

    let output = format!("{batch:?}");

    assert!(!output.contains("Secret123!"));
    assert!(output.contains("[redacted]"));
}

#[test]
fn sync_channel_serializes_with_stable_names() {
    assert_eq!(
        serde_json::to_string(&SyncChannel::Directory).unwrap(),
        "\"directory\""
    );
    assert_eq!(
        serde_json::to_string(&SyncChannel::Credential).unwrap(),
        "\"credential\""
    );
}
```

- [ ] **Step: Run contract tests and confirm failure**

Run: `cargo test -p adss-contract --test sync_contract`

Expected: compile failure for missing `DirectoryBatch`, `CredentialBatch`, `DirectoryPlan`, `DomainDirectoryConfig`, and `SyncChannel`.

- [ ] **Step: Implement the MVP contract types**

In `crates/adss-contract/src/lib.rs`, replace legacy public types with these concrete MVP types while preserving `serde` derives:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

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
}

impl DomainDirectoryConfig {
    pub fn example() -> Self {
        Self {
            domain_id: "domain-a".to_string(),
            mirror_root_dn: "OU=Mirror,DC=example,DC=com".to_string(),
            quarantine_ou_dn: "OU=Quarantine,DC=example,DC=com".to_string(),
            upn_suffix: "example.com".to_string(),
            employee_id_attribute: "employeeID".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryBatch {
    pub target_revision: u64,
    pub organizational_units: Vec<OrganizationalUnit>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    pub has_more: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialEntry {
    pub employee_id: String,
    pub plaintext_password: String,
    pub changed_revision: u64,
}

impl fmt::Debug for CredentialEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEntry")
            .field("employee_id", &self.employee_id)
            .field("plaintext_password", &"[redacted]")
            .field("changed_revision", &self.changed_revision)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialBatch {
    pub target_revision: u64,
    pub credentials: Vec<CredentialEntry>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncChannel {
    Directory,
    Credential,
}
```

Add request and response contracts:

```rust
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
```

Add directory operations:

```rust
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
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryPlan {
    pub target_revision: u64,
    pub operations: Vec<DirectoryOperation>,
}
```

Implement `DirectoryPlan::from_batch()` so it emits OU operations, active users, active user placement, groups, group members, then disabled user quarantine. Use `DomainDirectoryConfig.quarantine_ou_dn` as the target for disabled users.

- [ ] **Step: Verify contract crate**

Run: `cargo test -p adss-contract --test sync_contract`

Expected: all four tests pass.

- [ ] **Step: Commit contract model**

Run:

```powershell
git add crates/adss-contract/src/lib.rs crates/adss-contract/tests/sync_contract.rs
git commit -m "重建同步契约模型"
```

## Task Persistence Facts

**Files:**
- Modify: `crates/adss-persistence/src/lib.rs`
- Modify: `crates/adss-persistence/tests/orm_repository_contract.rs`

- [ ] **Step: Write repository contract tests**

Replace `crates/adss-persistence/tests/orm_repository_contract.rs` with tests for the MVP persistence API:

```rust
use adss_contract::{Group, OrganizationalUnit, User, UserStatus};
use adss_persistence::{
    CredentialRecord, DomainRecord, MvpRepository, UserCredentialInput, UserDirectoryPatch,
};

#[tokio::test]
async fn repository_updates_directory_objects_in_one_revision() {
    let repository = sqlite_repository().await;

    let revision = repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                employee_id: "1001".to_string(),
                username: "zhangsan".to_string(),
                display_name: "张三".to_string(),
                email: Some("zhangsan@example.com".to_string()),
                mobile: None,
                telephone: None,
                organizational_unit_id: "ou-rd".to_string(),
                status: UserStatus::Active,
            }],
            vec![Group {
                id: "dev".to_string(),
                name: "Developers".to_string(),
                member_employee_ids: vec!["1001".to_string()],
                changed_revision: 0,
            }],
        )
        .await
        .unwrap();

    let batch = repository.list_directory_changed_after("domain-a", 0, false, 100).await.unwrap();

    assert_eq!(revision, 1);
    assert_eq!(batch.target_revision, 1);
    assert_eq!(batch.organizational_units[0].changed_revision, 1);
    assert_eq!(batch.users[0].changed_revision, 1);
    assert_eq!(batch.groups[0].changed_revision, 1);
}

#[tokio::test]
async fn repository_returns_current_state_after_multiple_updates() {
    let repository = sqlite_repository().await;

    repository.seed_domain(domain()).await.unwrap();
    repository
        .upsert_directory(Vec::new(), vec![user_patch("1001", "first@example.com")], Vec::new())
        .await
        .unwrap();
    repository
        .upsert_directory(Vec::new(), vec![user_patch("1001", "latest@example.com")], Vec::new())
        .await
        .unwrap();

    let batch = repository.list_directory_changed_after("domain-a", 0, false, 100).await.unwrap();

    assert_eq!(batch.users.len(), 1);
    assert_eq!(batch.users[0].email.as_deref(), Some("latest@example.com"));
    assert_eq!(batch.target_revision, 2);
}

#[tokio::test]
async fn repository_updates_domain_revision_only_on_confirmed_channel() {
    let repository = sqlite_repository().await;
    repository.seed_domain(domain()).await.unwrap();

    repository.confirm_directory_revision("domain-a", 7).await.unwrap();
    repository.confirm_credential_revision("domain-a", 3).await.unwrap();
    let domain = repository.get_domain("domain-a").await.unwrap().unwrap();

    assert_eq!(domain.applied_directory_revision, 7);
    assert_eq!(domain.applied_credential_revision, 3);
}

#[tokio::test]
async fn repository_stores_only_current_credential() {
    let repository = sqlite_repository().await;

    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:first".to_string(),
            password_verifier: "verify:first".to_string(),
        })
        .await
        .unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:latest".to_string(),
            password_verifier: "verify:latest".to_string(),
        })
        .await
        .unwrap();

    let credentials = repository.list_credentials_changed_after(0, false, 100).await.unwrap();

    assert_eq!(credentials.credentials.len(), 1);
    assert_eq!(credentials.credentials[0].employee_id, "1001");
    assert_eq!(
        repository.get_credential_record("1001").await.unwrap().unwrap().password_ciphertext,
        "cipher:latest"
    );
}

async fn sqlite_repository() -> MvpRepository {
    let repository = MvpRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain()).await.unwrap();
    repository
}

fn domain() -> DomainRecord {
    DomainRecord {
        id: "domain-a".to_string(),
        name: "Domain A".to_string(),
        enabled: true,
        mirror_root_dn: "OU=Mirror,DC=a,DC=example,DC=com".to_string(),
        quarantine_ou_dn: "OU=Quarantine,DC=a,DC=example,DC=com".to_string(),
        upn_suffix: "a.example.com".to_string(),
        employee_id_attribute: "employeeID".to_string(),
        agent_key_hash: "hash:agent-key".to_string(),
        applied_directory_revision: 0,
        applied_credential_revision: 0,
    }
}

fn user_patch(employee_id: &str, email: &str) -> UserDirectoryPatch {
    UserDirectoryPatch {
        employee_id: employee_id.to_string(),
        username: employee_id.to_string(),
        display_name: employee_id.to_string(),
        email: Some(email.to_string()),
        mobile: None,
        telephone: None,
        organizational_unit_id: "ou-rd".to_string(),
        status: UserStatus::Active,
    }
}
```

- [ ] **Step: Run repository tests and confirm failure**

Run: `cargo test -p adss-persistence --test orm_repository_contract`

Expected: compile failure for missing `MvpRepository`, `DomainRecord`, `UserDirectoryPatch`, and `UserCredentialInput`.

- [ ] **Step: Implement MVP repository schema**

In `crates/adss-persistence/src/lib.rs`, replace the old entities with SeaORM entities for:

```text
sync_metadata(key, directory_revision, credential_revision)
organizational_units(id, name, parent_id, changed_revision)
users(employee_id, username, display_name, email, mobile, telephone, organizational_unit_id, status, changed_revision)
groups(id, name, member_employee_ids_json, changed_revision)
user_credentials(employee_id, password_ciphertext, password_verifier, changed_revision)
domains(id, name, enabled, mirror_root_dn, quarantine_ou_dn, upn_suffix, employee_id_attribute, agent_key_hash, applied_directory_revision, applied_credential_revision)
```

Use `CREATE TABLE IF NOT EXISTS` for the first pass, matching the existing repository style. Store member lists as JSON in `member_employee_ids_json`; do not add a separate membership table.

- [ ] **Step: Implement repository methods**

Expose `pub struct MvpRepository` and these concrete methods:

```rust
impl MvpRepository {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self>;
    pub async fn initialize_schema(&self) -> anyhow::Result<()>;
    pub async fn seed_domain(&self, domain: DomainRecord) -> anyhow::Result<()>;
    pub async fn get_domain(&self, domain_id: &str) -> anyhow::Result<Option<DomainRecord>>;
    pub async fn upsert_directory(
        &self,
        ous: Vec<OrganizationalUnit>,
        users: Vec<UserDirectoryPatch>,
        groups: Vec<Group>,
    ) -> anyhow::Result<u64>;
    pub async fn list_directory_changed_after(
        &self,
        domain_id: &str,
        applied_revision: u64,
        rebuild: bool,
        limit: usize,
    ) -> anyhow::Result<DirectoryBatch>;
    pub async fn change_user_password(&self, input: UserCredentialInput) -> anyhow::Result<u64>;
    pub async fn get_credential_record(
        &self,
        employee_id: &str,
    ) -> anyhow::Result<Option<CredentialRecord>>;
    pub async fn list_credentials_changed_after(
        &self,
        applied_revision: u64,
        rebuild: bool,
        limit: usize,
    ) -> anyhow::Result<CredentialBatch>;
    pub async fn confirm_directory_revision(&self, domain_id: &str, revision: u64) -> anyhow::Result<()>;
    pub async fn confirm_credential_revision(&self, domain_id: &str, revision: u64) -> anyhow::Result<()>;
}
```

Use database transactions for `upsert_directory()` and `change_user_password()`. Confirm methods must reject revisions lower than the stored applied revision and revisions higher than the current global channel revision.

- [ ] **Step: Verify repository behavior**

Run: `cargo test -p adss-persistence --test orm_repository_contract`

Expected: all repository tests pass.

- [ ] **Step: Commit persistence model**

Run:

```powershell
git add crates/adss-persistence/src/lib.rs crates/adss-persistence/tests/orm_repository_contract.rs
git commit -m "重建同步持久化模型"
```

## Task Server MVP API

**Files:**
- Modify: `crates/adss-server/src/lib.rs`
- Modify: `crates/adss-server/src/main.rs`
- Modify: `crates/adss-server/tests/api_contract.rs`

- [ ] **Step: Write server API tests**

Replace `crates/adss-server/tests/api_contract.rs` with tests for:

```rust
#[tokio::test]
async fn user_update_returns_changed_directory_objects_only() {
    let app = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    let response = agent_sync(&app, 0, 0, false, false).await;

    assert_eq!(response.directory.target_revision, 1);
    assert_eq!(response.directory.users.len(), 1);
    assert_eq!(response.directory.users[0].email.as_deref(), Some("zhangsan@example.com"));
    assert!(response.credentials.credentials.is_empty());
}

#[tokio::test]
async fn directory_confirm_advances_only_directory_channel() {
    let app = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 1, true).await;
    let response = agent_sync(&app, 1, 0, false, false).await;

    assert!(response.directory.users.is_empty());
    assert_eq!(response.directory.target_revision, 1);
}

#[tokio::test]
async fn failed_confirm_does_not_advance_revision() {
    let app = test_app().await;

    patch_user(&app, "1001", "zhangsan@example.com").await;
    confirm(&app, SyncChannel::Directory, 1, false).await;
    let response = agent_sync(&app, 0, 0, false, false).await;

    assert_eq!(response.directory.target_revision, 1);
    assert_eq!(response.directory.users.len(), 1);
}

#[tokio::test]
async fn password_change_returns_credentials_only_to_agent_sync() {
    let app = test_app().await;

    login_and_change_password(&app, "1001", "OldPass123!", "NewPass123!").await;
    let response = agent_sync(&app, 0, 0, false, false).await;

    assert!(response.directory.users.is_empty());
    assert_eq!(response.credentials.target_revision, 1);
    assert_eq!(response.credentials.credentials[0].employee_id, "1001");
    assert_eq!(response.credentials.credentials[0].plaintext_password, "NewPass123!");
}

#[tokio::test]
async fn sync_requires_matching_agent_key() {
    let app = test_app().await;

    let response = request_with_agent_key(&app, "wrong-key").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

Reuse the existing Axum request helper pattern at the bottom of the file. Seed test data through `MvpRepository` so the server tests exercise repository-backed behavior only.

- [ ] **Step: Run server tests and confirm failure**

Run: `cargo test -p adss-server --test api_contract`

Expected: compile failure for removed old response types and missing MVP routes.

- [ ] **Step: Replace AppState with repository-backed state**

In `crates/adss-server/src/lib.rs`, define:

```rust
#[derive(Clone)]
pub struct AppState {
    repository: MvpRepository,
    batch_limit: usize,
}
```

Remove the in-memory `Store`, seeded production path, agent registration, password task creation, drift report, audit event list, and old poll/report handlers. Keep only behavior required by the MVP.

- [ ] **Step: Implement MVP routes**

Build router routes:

```text
POST /api/auth/login
PATCH /api/users/{employee_id}
POST /api/users/{employee_id}/password
POST /api/agent/sync
POST /api/agent/confirm
```

Handler behavior:

- `login`: verify `password_verifier` for the employee and return `UserLoginResponse`.
- `PATCH /api/users/{employee_id}`: update user directory fields in one repository transaction and return the new directory revision.
- `POST /api/users/{employee_id}/password`: verify current password, update verifier and encrypted password in one transaction, and return `PasswordChangeResponse`.
- `POST /api/agent/sync`: authenticate agent key hash, return directory and credential batches independently.
- `POST /api/agent/confirm`: authenticate, update only the requested channel when `success == true`, and leave revisions unchanged when `success == false`.

For MVP test encryption, implement private helpers:

```rust
fn seal_password_for_storage(password: &str) -> String {
    format!("sealed:{password}")
}

fn open_password_for_agent(ciphertext: &str) -> Option<String> {
    ciphertext.strip_prefix("sealed:").map(str::to_string)
}

fn password_verifier(password: &str) -> String {
    format!("verifier:{}", password.len())
}
```

These helpers are test-only stand-ins for the KMS boundary and must stay private. Tests must assert that plaintext is not stored in `password_verifier`; production-grade password hashing is a separate implementation step before deployment.

- [ ] **Step: Require database-backed startup**

In `crates/adss-server/src/main.rs`, require `ADSS_DATABASE_URL`. Exit with a clear error if it is missing:

```rust
let database_url = config
    .database_url
    .as_deref()
    .ok_or_else(|| anyhow::anyhow!("ADSS_DATABASE_URL is required for the MVP server"))?;
let repository = MvpRepository::connect(database_url).await?;
repository.initialize_schema().await?;
let state = AppState::new(repository);
```

- [ ] **Step: Verify server API behavior**

Run: `cargo test -p adss-server --test api_contract`

Expected: all MVP server API tests pass.

- [ ] **Step: Commit server MVP API**

Run:

```powershell
git add crates/adss-server/src/lib.rs crates/adss-server/src/main.rs crates/adss-server/tests/api_contract.rs
git commit -m "实现 MVP 同步控制面"
```

## Task Agent Runtime

**Files:**
- Modify: `crates/adss-agent/src/lib.rs`
- Modify: `crates/adss-agent/src/main.rs`
- Modify: `crates/adss-agent/tests/http_client_contract.rs`
- Modify: `crates/adss-agent/tests/runtime_contract.rs`
- Modify: `crates/adss-agent/tests/execution_contract.rs`

- [ ] **Step: Write Agent runtime tests**

Replace runtime tests with these scenarios:

```rust
#[tokio::test]
async fn runtime_confirms_directory_only_after_full_success() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5));
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = AgentRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 5);
    assert_eq!(runtime.local_state().applied_credential_revision, 0);
    assert_eq!(runtime.control_plane().confirmed_directory_revision(), Some(5));
}

#[tokio::test]
async fn runtime_does_not_confirm_failed_directory_batch() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5));
    let directory = RecordingDirectory::fails_directory("ensure_user");
    let state = MemoryLocalState::default();
    let mut runtime = AgentRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(runtime.control_plane().confirmed_directory_revision(), None);
}

#[tokio::test]
async fn credential_failure_does_not_block_directory_confirmation() {
    let control = RecordingControlPlane::with_batches(directory_batch(5), credential_batch(2));
    let directory = RecordingDirectory::fails_password_for("1001");
    let state = MemoryLocalState::default();
    let mut runtime = AgentRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 5);
    assert_eq!(runtime.local_state().applied_credential_revision, 0);
}
```

- [ ] **Step: Write HTTP client tests**

In `crates/adss-agent/tests/http_client_contract.rs`, assert that `HttpControlPlaneClient` posts to:

```text
/api/agent/sync
/api/agent/confirm
```

and includes `x-adss-agent-key`. Assert it serializes `applied_directory_revision`, `applied_credential_revision`, and both rebuild flags.

- [ ] **Step: Run Agent tests and confirm failure**

Run: `cargo test -p adss-agent`

Expected: compile failure against the legacy `ControlPlaneClient`, `AgentCursor`, and password-task runtime.

- [ ] **Step: Implement Agent control-plane and local state**

Define:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalRevisionState {
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

pub trait LocalStateStore {
    fn load(&self) -> anyhow::Result<LocalRevisionState>;
    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ControlPlaneClient {
    async fn sync(&self, request: AgentSyncRequest) -> anyhow::Result<AgentSyncResponse>;
    async fn confirm(&self, request: AgentConfirmRequest) -> anyhow::Result<AgentConfirmResponse>;
}
```

Implement `FileLocalStateStore` using a JSON file and atomic replace. The file contains exactly:

```json
{"applied_directory_revision":0,"applied_credential_revision":0}
```

- [ ] **Step: Implement independent channel runtime**

`AgentRuntime::run_once()` must:

- Load local state.
- Call `sync()`.
- Execute directory batch when it contains objects.
- Confirm directory only after full success.
- Execute credential batch even when directory failed.
- Confirm credential only after full success.
- Save a channel revision only after the corresponding confirm call returns `accepted == true`.

- [ ] **Step: Update directory execution**

Replace `AdExecutor` with `DirectoryExecutor` that consumes `DirectoryPlan`. Keep dry-run implementation for tests and local smoke runs, but name it `DryRunDirectoryClient` and make it explicit in configuration.

- [ ] **Step: Update Agent main loop**

In `crates/adss-agent/src/main.rs`, load:

```text
ADSS_SERVER_URL
ADSS_DOMAIN_ID
ADSS_AGENT_KEY
ADSS_AGENT_STATE_PATH
ADSS_AGENT_INTERVAL_SECONDS
ADSS_AGENT_DRY_RUN
```

Run an infinite loop with `tokio::time::sleep()` between iterations. Keep the existing guard that refuses non-dry-run until a real LDAPS client is implemented.

- [ ] **Step: Verify Agent behavior**

Run: `cargo test -p adss-agent`

Expected: all Agent tests pass.

- [ ] **Step: Commit Agent runtime**

Run:

```powershell
git add crates/adss-agent/src/lib.rs crates/adss-agent/src/main.rs crates/adss-agent/tests/runtime_contract.rs crates/adss-agent/tests/http_client_contract.rs crates/adss-agent/tests/execution_contract.rs
git commit -m "实现 Agent MVP 同步运行时"
```

## Task Workspace Integration

**Files:**
- Modify: `Cargo.toml`
- Modify: crate `Cargo.toml` files if dependencies change
- Modify: `docs/api.md`
- Modify: `docs/database.md`
- Modify: `docs/sync-protocol.md`
- Modify: `docs/operations.md`
- Modify: `docs/security.md`
- Modify: `docs/architecture.md`

- [ ] **Step: Remove unused dependencies**

Run:

```powershell
cargo machete
```

If `cargo machete` is unavailable, inspect each crate manifest manually and remove dependencies no longer used by the MVP code. Keep `sea-orm`, `axum`, `reqwest`, `tokio`, `serde`, `serde_json`, `anyhow`, `thiserror`, and `async-trait` only where used.

- [ ] **Step: Update public docs**

Update docs to describe only the MVP behavior:

- `docs/api.md`: document `/api/auth/login`, `/api/users/{employee_id}`, `/api/users/{employee_id}/password`, `/api/agent/sync`, and `/api/agent/confirm`.
- `docs/database.md`: document `sync_metadata`, `organizational_units`, `users`, `groups`, `user_credentials`, and `domains`.
- `docs/sync-protocol.md`: document independent directory and credential batches, rebuild flags, and整批确认.
- `docs/operations.md`: document required `ADSS_DATABASE_URL`, Agent state path, interval, dry-run guard, and TLS/LDAPS prerequisites.
- `docs/security.md`: document center-only password changes, verifier versus ciphertext, Agent key hash, TLS plaintext credential response, and AD Change Password denial.
- `docs/architecture.md`: replace old Store/task/drift language with the final-state incremental model.

- [ ] **Step: Run workspace formatting**

Run: `cargo fmt --all`

Expected: command exits successfully and only formats Rust files touched by the tasks.

- [ ] **Step: Run full workspace tests**

Run: `cargo test --workspace`

Expected: all tests pass.

- [ ] **Step: Run required Rust lint**

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: no warnings.

- [ ] **Step: Commit docs and integration cleanup**

Run:

```powershell
git add Cargo.toml crates/adss-agent/Cargo.toml crates/adss-contract/Cargo.toml crates/adss-persistence/Cargo.toml crates/adss-server/Cargo.toml docs/api.md docs/database.md docs/sync-protocol.md docs/operations.md docs/security.md docs/architecture.md
git commit -m "同步 MVP 文档和依赖"
```

## Self Review

- Spec coverage: the plan covers the current fact model, object `changed_revision`, directory and credential revisions, per-domain applied revisions, Agent pull,整批确认, local revision persistence, center-only password changes, TLS plaintext credential response, and docs alignment.
- Scope control: the plan excludes multi-Agent coordination, drift, registration tokens, task queues, audit platforms, mTLS, physical deletion, administrator password reset, and real LDAPS implementation beyond the existing dry-run boundary.
- Verification: each code task starts with failing tests and ends with package tests; final integration runs `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --all-targets --all-features -- -D warnings`.
