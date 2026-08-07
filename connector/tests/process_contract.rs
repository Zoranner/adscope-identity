use adss_connector::{
    ConnectorRuntime, ControlPlaneClient, DirectoryBatchSession, DirectoryClient,
    DirectoryExecutionContext, LocalRevisionState, LocalStateStore, run_connector_loop,
};
use adss_protocol::{
    ConnectorConfirmRequest, ConnectorConfirmResponse, ConnectorSyncRequest, ConnectorSyncResponse,
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DomainDirectoryConfig,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, watch};

#[tokio::test]
async fn connector_loop_finishes_in_flight_run_then_stops_before_next_interval() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let calls = Arc::new(Mutex::new(0_u32));
    let control = BlockingControlPlane {
        started: Arc::clone(&started),
        release: Arc::clone(&release),
        calls: Arc::clone(&calls),
    };
    let mut runtime =
        ConnectorRuntime::new("domain-a", control, NoopDirectory, MemoryState::default());
    let (stop_tx, stop_rx) = watch::channel(false);

    let task = tokio::spawn(async move {
        run_connector_loop(&mut runtime, Duration::from_millis(10), stop_rx, |_| {}).await
    });

    started.notified().await;
    stop_tx.send(true).unwrap();
    assert!(!task.is_finished());
    release.notify_one();
    task.await.unwrap();

    assert_eq!(*calls.lock().unwrap(), 1);
}

struct BlockingControlPlane {
    started: Arc<Notify>,
    release: Arc<Notify>,
    calls: Arc<Mutex<u32>>,
}

#[async_trait]
impl ControlPlaneClient for BlockingControlPlane {
    async fn sync(&self, _request: ConnectorSyncRequest) -> anyhow::Result<ConnectorSyncResponse> {
        *self.calls.lock().unwrap() += 1;
        self.started.notify_one();
        self.release.notified().await;
        Ok(ConnectorSyncResponse {
            directory: empty_directory_batch(),
            credentials: empty_credential_batch(),
            directory_config: DomainDirectoryConfig::example(),
        })
    }

    async fn confirm(
        &self,
        _request: ConnectorConfirmRequest,
    ) -> anyhow::Result<ConnectorConfirmResponse> {
        unreachable!("empty batches are not confirmed")
    }
}

struct NoopDirectory;

#[async_trait]
impl DirectoryClient for NoopDirectory {
    type Batch = NoopDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        Ok(NoopDirectoryBatch)
    }
}

struct NoopDirectoryBatch;

#[async_trait]
impl DirectoryBatchSession for NoopDirectoryBatch {
    async fn apply(
        &mut self,
        _operation: &DirectoryOperation,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_password(
        &mut self,
        _credential: &CredentialEntry,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct MemoryState(Mutex<LocalRevisionState>);

impl LocalStateStore for MemoryState {
    fn load(&self) -> anyhow::Result<LocalRevisionState> {
        Ok(*self.0.lock().unwrap())
    }

    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()> {
        *self.0.lock().unwrap() = state;
        Ok(())
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
