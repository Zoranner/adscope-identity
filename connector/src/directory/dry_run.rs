use adss_protocol::{CredentialEntry, DirectoryOperation};
use async_trait::async_trait;

use super::{DirectoryClient, DirectoryExecutionContext};

pub struct DryRunDirectoryClient;

#[async_trait]
impl DirectoryClient for DryRunDirectoryClient {
    async fn apply(
        &self,
        _operation: &DirectoryOperation,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_password(
        &self,
        _credential: &CredentialEntry,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
