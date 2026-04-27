use adss_agent::{AdExecutor, DirectoryClient, execute_reconcile_plan};
use adss_contract::{AdOperation, AdOperationKind, ReconcilePlan};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn executor_runs_reconcile_operations_in_plan_order() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let client = RecordingDirectoryClient {
        calls: Arc::clone(&calls),
    };
    let executor = AdExecutor::new(client);
    let plan = ReconcilePlan {
        target_version: 11,
        operations: vec![
            AdOperation::new(AdOperationKind::EnsureOu, "OU=研发中心"),
            AdOperation::new(AdOperationKind::EnsureGroup, "CN=rust-dev,OU=研发中心"),
            AdOperation::new(AdOperationKind::EnsureUser, "CN=张三,OU=研发中心"),
            AdOperation::new(AdOperationKind::EnsureUserPlacement, "CN=张三,OU=研发中心"),
            AdOperation::new(
                AdOperationKind::EnsureGroupMembership,
                "CN=rust-dev,OU=研发中心",
            ),
        ],
    };

    let summary = executor.execute(&plan).await.unwrap();

    assert_eq!(summary.succeeded, 5);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            AdOperationKind::EnsureOu,
            AdOperationKind::EnsureGroup,
            AdOperationKind::EnsureUser,
            AdOperationKind::EnsureUserPlacement,
            AdOperationKind::EnsureGroupMembership,
        ]
    );
}

#[tokio::test]
async fn failed_operation_is_reported_without_running_later_operations() {
    let client = FailingDirectoryClient;
    let plan = ReconcilePlan {
        target_version: 12,
        operations: vec![
            AdOperation::new(AdOperationKind::EnsureOu, "OU=研发中心"),
            AdOperation::new(AdOperationKind::EnsureUser, "CN=张三,OU=研发中心"),
        ],
    };

    let summary = execute_reconcile_plan(&client, &plan).await;

    assert_eq!(summary.succeeded, 0);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.skipped, 1);
}

struct RecordingDirectoryClient {
    calls: Arc<Mutex<Vec<AdOperationKind>>>,
}

#[async_trait]
impl DirectoryClient for RecordingDirectoryClient {
    async fn apply(&self, operation: &AdOperation) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(operation.kind);
        Ok(())
    }
}

struct FailingDirectoryClient;

#[async_trait]
impl DirectoryClient for FailingDirectoryClient {
    async fn apply(&self, _operation: &AdOperation) -> anyhow::Result<()> {
        anyhow::bail!("LDAPS permission denied")
    }
}
