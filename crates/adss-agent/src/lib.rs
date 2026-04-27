use adss_contract::{AdOperation, ReconcilePlan, SyncSummary};
use async_trait::async_trait;

#[async_trait]
pub trait DirectoryClient {
    async fn apply(&self, operation: &AdOperation) -> anyhow::Result<()>;
}

pub struct AdExecutor<C> {
    client: C,
}

impl<C> AdExecutor<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> AdExecutor<C>
where
    C: DirectoryClient + Sync,
{
    pub async fn execute(&self, plan: &ReconcilePlan) -> anyhow::Result<SyncSummary> {
        Ok(execute_reconcile_plan(&self.client, plan).await)
    }
}

pub async fn execute_reconcile_plan<C>(client: &C, plan: &ReconcilePlan) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, operation) in plan.operations.iter().enumerate() {
        match client.apply(operation).await {
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
