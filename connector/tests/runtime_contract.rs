use adscope_connector::{
    ConnectorRuntime, ControlPlaneClient, DirectoryBatchSession, DirectoryClient,
    DirectoryExecutionContext, FileLocalStateStore, LocalRevisionState, LocalStateStore,
};
use adscope_protocol::{
    ConnectorConfirmRequest, ConnectorConfirmResponse, ConnectorSyncRequest, ConnectorSyncResponse,
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DomainDirectoryConfig,
    Group, OrganizationalUnit, SyncChannel, User, UserStatus,
};
use async_trait::async_trait;
use std::fs;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn runtime_confirms_directory_only_after_full_success() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5));
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    let summary = runtime.run_once().await.unwrap();

    assert_eq!(
        runtime.local_state(),
        LocalRevisionState {
            applied_directory_revision: 5,
            applied_credential_revision: 0,
        }
    );
    assert_eq!(
        runtime.control_plane().confirmed_directory_revision(),
        Some(5)
    );
    assert!(summary.directory_failure.is_none());
    assert_eq!(
        runtime.control_plane().confirmed_credential_revision(),
        None
    );
}

#[tokio::test]
async fn runtime_confirms_a_successful_directory_page_at_its_batch_revision() {
    let mut batch = directory_batch(5);
    batch.server_revision = 7;
    batch.has_more = true;
    let control = RecordingControlPlane::with_directory_batch(batch);
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a", control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 5);
    assert_eq!(
        runtime.control_plane().confirmed_directory_revision(),
        Some(5)
    );
}

#[tokio::test]
async fn runtime_does_not_confirm_failed_directory_batch() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5));
    let directory = RecordingDirectory::fails_directory("ensure_user");
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    let summary = runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(runtime.control_plane().confirmed_directory_revision(), None);
    assert_eq!(
        runtime
            .control_plane()
            .failed_confirm_error_code(SyncChannel::Directory),
        Some("directory_execution_failed".to_string())
    );
    let failure = summary.directory_failure.unwrap();
    assert_eq!(failure.operation, "ensure_user");
    assert_eq!(failure.subject, "1001");
    assert!(failure.detail.contains("directory operation failed"));
}

#[tokio::test]
async fn session_open_failure_confirms_directory_without_advancing_revision() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5));
    let directory = RecordingDirectory::fails_to_open();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a", control, directory, state);

    let summary = runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(
        runtime
            .control_plane()
            .failed_confirm_error_code(SyncChannel::Directory),
        Some("directory_execution_failed".to_string())
    );
    assert_eq!(
        summary.directory_failure.unwrap().operation,
        "open_directory_batch"
    );
}

#[tokio::test]
async fn empty_batches_do_not_open_directory_sessions() {
    let control =
        RecordingControlPlane::with_batches(empty_directory_batch(), empty_credential_batch());
    let directory = RecordingDirectory::succeeds();
    let opened_batches = Arc::clone(&directory.opened_batches);
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a", control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(*opened_batches.lock().unwrap(), 0);
}

#[tokio::test]
async fn credential_failure_does_not_block_directory_confirmation() {
    let control = RecordingControlPlane::with_batches(directory_batch(5), credential_batch(2));
    let directory = RecordingDirectory::fails_password_for("1001");
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    let summary = runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 5);
    assert_eq!(runtime.local_state().applied_credential_revision, 0);
    assert_eq!(
        runtime.control_plane().confirmed_directory_revision(),
        Some(5)
    );
    assert_eq!(
        runtime.control_plane().confirmed_credential_revision(),
        None
    );
    assert_eq!(
        runtime
            .control_plane()
            .failed_confirm_error_code(SyncChannel::Credential),
        Some("credential_execution_failed".to_string())
    );
    let failure = summary.credential_failure.unwrap();
    assert_eq!(failure.operation, "set_password");
    assert_eq!(failure.subject, "1001");
    assert!(failure.detail.contains("password failed"));
    assert!(!failure.detail.contains("NewPass123!"));
}

#[tokio::test]
async fn confirm_failure_does_not_advance_local_revision() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5))
        .with_confirm_failure(SyncChannel::Directory);
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    assert!(runtime.run_once().await.is_err());

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(
        runtime.control_plane().confirmed_directory_revision(),
        Some(5)
    );
}

#[tokio::test]
async fn invalid_file_state_rebuilds_both_channels_and_overwrites_after_confirm() {
    let path = temp_state_path("rebuild-invalid-json");
    fs::write(&path, "not-json").unwrap();
    let control = RecordingControlPlane::with_batches(directory_batch(5), credential_batch(2))
        .with_expected_sync_request(ExpectedSyncRequest {
            applied_directory_revision: 0,
            applied_credential_revision: 0,
            rebuild_directory: true,
            rebuild_credentials: true,
        });
    let directory = RecordingDirectory::succeeds();
    let state = FileLocalStateStore::new(&path);
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(
        runtime.local_state(),
        LocalRevisionState {
            applied_directory_revision: 5,
            applied_credential_revision: 2,
        }
    );

    let _ = fs::remove_file(path);
}

#[tokio::test]
async fn accepted_false_does_not_advance_local_revision() {
    let control = RecordingControlPlane::with_directory_batch(directory_batch(5))
        .with_confirm_rejection(SyncChannel::Directory);
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(runtime.local_state_store().save_count(), 0);
    assert_eq!(
        runtime.control_plane().confirmed_directory_revision(),
        Some(5)
    );
}

#[tokio::test]
async fn directory_plan_error_sends_failed_directory_confirm() {
    let control = RecordingControlPlane::with_directory_batch(invalid_directory_batch(5));
    let directory = RecordingDirectory::succeeds();
    let state = MemoryLocalState::default();
    let mut runtime = ConnectorRuntime::new("domain-a".to_string(), control, directory, state);

    runtime.run_once().await.unwrap();

    assert_eq!(runtime.local_state().applied_directory_revision, 0);
    assert_eq!(
        runtime
            .control_plane()
            .failed_confirm_error_code(SyncChannel::Directory),
        Some("directory_plan_failed".to_string())
    );
}

#[test]
fn file_local_state_store_round_trips_revisions() {
    let path = temp_state_path("round-trip");
    let store = FileLocalStateStore::new(&path);

    store
        .save(LocalRevisionState {
            applied_directory_revision: 8,
            applied_credential_revision: 13,
        })
        .unwrap();

    assert_eq!(
        store.load().unwrap(),
        LocalRevisionState {
            applied_directory_revision: 8,
            applied_credential_revision: 13,
        }
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_local_state_store_rejects_invalid_json() {
    let path = temp_state_path("invalid-json");
    fs::write(&path, "not-json").unwrap();
    let store = FileLocalStateStore::new(&path);

    assert!(store.load().is_err());

    let _ = fs::remove_file(path);
}

#[derive(Clone)]
struct RecordingControlPlane {
    sync_response: Arc<ConnectorSyncResponse>,
    confirms: Arc<Mutex<Vec<ConnectorConfirmRequest>>>,
    fail_confirm_channel: Option<SyncChannel>,
    reject_confirm_channel: Option<SyncChannel>,
    expected_sync_request: ExpectedSyncRequest,
}

impl RecordingControlPlane {
    fn with_directory_batch(directory: DirectoryBatch) -> Self {
        Self::with_batches(directory, empty_credential_batch())
    }

    fn with_batches(directory: DirectoryBatch, credentials: CredentialBatch) -> Self {
        Self {
            sync_response: Arc::new(ConnectorSyncResponse {
                directory,
                credentials,
                directory_config: DomainDirectoryConfig::example(),
            }),
            confirms: Arc::new(Mutex::new(Vec::new())),
            fail_confirm_channel: None,
            reject_confirm_channel: None,
            expected_sync_request: ExpectedSyncRequest::default(),
        }
    }

    fn with_confirm_failure(mut self, channel: SyncChannel) -> Self {
        self.fail_confirm_channel = Some(channel);
        self
    }

    fn with_confirm_rejection(mut self, channel: SyncChannel) -> Self {
        self.reject_confirm_channel = Some(channel);
        self
    }

    fn with_expected_sync_request(mut self, expected_sync_request: ExpectedSyncRequest) -> Self {
        self.expected_sync_request = expected_sync_request;
        self
    }

    fn confirmed_directory_revision(&self) -> Option<u64> {
        self.confirmed_revision(SyncChannel::Directory)
    }

    fn confirmed_credential_revision(&self) -> Option<u64> {
        self.confirmed_revision(SyncChannel::Credential)
    }

    fn confirmed_revision(&self, channel: SyncChannel) -> Option<u64> {
        self.confirms
            .lock()
            .unwrap()
            .iter()
            .find(|confirm| confirm.channel == channel && confirm.success)
            .map(|confirm| confirm.target_revision)
    }

    fn failed_confirm_error_code(&self, channel: SyncChannel) -> Option<String> {
        self.confirms
            .lock()
            .unwrap()
            .iter()
            .find(|confirm| confirm.channel == channel && !confirm.success)
            .and_then(|confirm| confirm.error_code.clone())
    }
}

#[async_trait]
impl ControlPlaneClient for RecordingControlPlane {
    async fn sync(&self, request: ConnectorSyncRequest) -> anyhow::Result<ConnectorSyncResponse> {
        assert_eq!(request.domain_id, "domain-a");
        assert_eq!(
            request.applied_directory_revision,
            self.expected_sync_request.applied_directory_revision
        );
        assert_eq!(
            request.applied_credential_revision,
            self.expected_sync_request.applied_credential_revision
        );
        assert_eq!(
            request.rebuild_directory,
            self.expected_sync_request.rebuild_directory
        );
        assert_eq!(
            request.rebuild_credentials,
            self.expected_sync_request.rebuild_credentials
        );

        Ok((*self.sync_response).clone())
    }

    async fn confirm(
        &self,
        request: ConnectorConfirmRequest,
    ) -> anyhow::Result<ConnectorConfirmResponse> {
        self.confirms.lock().unwrap().push(request.clone());
        if self.fail_confirm_channel == Some(request.channel) {
            anyhow::bail!("confirm failed");
        }

        Ok(ConnectorConfirmResponse {
            accepted: self.reject_confirm_channel != Some(request.channel),
        })
    }
}

#[derive(Clone, Copy, Default)]
struct ExpectedSyncRequest {
    applied_directory_revision: u64,
    applied_credential_revision: u64,
    rebuild_directory: bool,
    rebuild_credentials: bool,
}

#[derive(Clone)]
struct RecordingDirectory {
    fail_directory_kind: Option<&'static str>,
    fail_password_employee_id: Option<&'static str>,
    fail_open: bool,
    opened_batches: Arc<Mutex<u32>>,
}

impl RecordingDirectory {
    fn succeeds() -> Self {
        Self {
            fail_directory_kind: None,
            fail_password_employee_id: None,
            fail_open: false,
            opened_batches: Arc::new(Mutex::new(0)),
        }
    }

    fn fails_directory(kind: &'static str) -> Self {
        Self {
            fail_directory_kind: Some(kind),
            fail_password_employee_id: None,
            fail_open: false,
            opened_batches: Arc::new(Mutex::new(0)),
        }
    }

    fn fails_password_for(employee_id: &'static str) -> Self {
        Self {
            fail_directory_kind: None,
            fail_password_employee_id: Some(employee_id),
            fail_open: false,
            opened_batches: Arc::new(Mutex::new(0)),
        }
    }

    fn fails_to_open() -> Self {
        Self {
            fail_directory_kind: None,
            fail_password_employee_id: None,
            fail_open: true,
            opened_batches: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl DirectoryClient for RecordingDirectory {
    type Batch = RecordingDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        *self.opened_batches.lock().unwrap() += 1;
        if self.fail_open {
            anyhow::bail!("GSS-API bind failed");
        }
        Ok(RecordingDirectoryBatch(self.clone()))
    }
}

struct RecordingDirectoryBatch(RecordingDirectory);

#[async_trait]
impl DirectoryBatchSession for RecordingDirectoryBatch {
    async fn apply(
        &mut self,
        operation: &DirectoryOperation,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        if self.0.fail_directory_kind == Some(directory_kind_name(operation.kind)) {
            anyhow::bail!("directory operation failed");
        }

        Ok(())
    }

    async fn set_password(
        &mut self,
        credential: &CredentialEntry,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        if self.0.fail_password_employee_id == Some(credential.employee_id.as_str()) {
            anyhow::bail!("password failed");
        }

        Ok(())
    }
}

#[derive(Clone, Default)]
struct MemoryLocalState {
    state: Arc<Mutex<LocalRevisionState>>,
    save_count: Arc<Mutex<u32>>,
}

impl MemoryLocalState {
    fn save_count(&self) -> u32 {
        *self.save_count.lock().unwrap()
    }
}

impl LocalStateStore for MemoryLocalState {
    fn load(&self) -> anyhow::Result<LocalRevisionState> {
        Ok(*self.state.lock().unwrap())
    }

    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()> {
        *self.state.lock().unwrap() = state;
        *self.save_count.lock().unwrap() += 1;
        Ok(())
    }
}

fn directory_batch(batch_revision: u64) -> DirectoryBatch {
    DirectoryBatch {
        server_revision: batch_revision,
        batch_revision,
        organizational_units: vec![OrganizationalUnit {
            id: "ou-rd".to_string(),
            name: "Research".to_string(),
            parent_id: None,
            changed_revision: batch_revision,
        }],
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: vec![User {
            employee_id: "1001".to_string(),
            username: "zhangsan".to_string(),
            display_name: "Zhang San".to_string(),
            email: Some("zhangsan@example.com".to_string()),
            mobile: None,
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Active,
            changed_revision: batch_revision,
        }],
        groups: vec![Group {
            id: "developers".to_string(),
            name: "Developers".to_string(),
            organizational_unit_id: "ou-rd".to_string(),
            member_employee_ids: vec!["1001".to_string()],
            changed_revision: batch_revision,
        }],
        has_more: false,
    }
}

fn invalid_directory_batch(batch_revision: u64) -> DirectoryBatch {
    DirectoryBatch {
        server_revision: batch_revision,
        batch_revision,
        organizational_units: vec![
            OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "Research".to_string(),
                parent_id: None,
                changed_revision: batch_revision,
            },
            OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "Duplicate Research".to_string(),
                parent_id: None,
                changed_revision: batch_revision,
            },
        ],
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    }
}

fn credential_batch(batch_revision: u64) -> CredentialBatch {
    CredentialBatch {
        server_revision: batch_revision,
        batch_revision,
        credentials: vec![CredentialEntry {
            employee_id: "1001".to_string(),
            plaintext_password: "NewPass123!".to_string(),
            status: UserStatus::Active,
            changed_revision: batch_revision,
        }],
        has_more: false,
    }
}

fn empty_directory_batch() -> DirectoryBatch {
    DirectoryBatch {
        server_revision: 0,
        batch_revision: 0,
        organizational_units: Vec::new(),
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    }
}

fn empty_credential_batch() -> CredentialBatch {
    CredentialBatch {
        server_revision: 0,
        batch_revision: 0,
        credentials: Vec::new(),
        has_more: false,
    }
}

fn directory_kind_name(kind: adscope_protocol::DirectoryOperationKind) -> &'static str {
    match kind {
        adscope_protocol::DirectoryOperationKind::EnsureOu => "ensure_ou",
        adscope_protocol::DirectoryOperationKind::EnsureUser => "ensure_user",
        adscope_protocol::DirectoryOperationKind::EnsureUserPlacement => "ensure_user_placement",
        adscope_protocol::DirectoryOperationKind::EnsureGroup => "ensure_group",
        adscope_protocol::DirectoryOperationKind::EnsureGroupMembers => "ensure_group_members",
        adscope_protocol::DirectoryOperationKind::DisableUser => "disable_user",
        adscope_protocol::DirectoryOperationKind::MoveUserToQuarantine => "move_user_to_quarantine",
    }
}

fn temp_state_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "adss-connector-{name}-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}
