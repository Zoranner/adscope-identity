use adss_protocol::{CredentialEntry, DirectoryOperation};
use async_trait::async_trait;

use super::{DirectoryBatchSession, DirectoryClient, DirectoryExecutionContext};

pub struct DryRunDirectoryClient;
pub struct DryRunDirectoryBatch;

#[async_trait]
impl DirectoryClient for DryRunDirectoryClient {
    type Batch = DryRunDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        Ok(DryRunDirectoryBatch)
    }
}

#[async_trait]
impl DirectoryBatchSession for DryRunDirectoryBatch {
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
