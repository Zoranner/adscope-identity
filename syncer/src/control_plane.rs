use adscope_protocol::{
    SyncerConfirmRequest, SyncerConfirmResponse, SyncerSyncRequest, SyncerSyncResponse,
};
use async_trait::async_trait;
use std::time::Duration;

#[async_trait]
pub trait ControlPlaneClient {
    async fn sync(&self, request: SyncerSyncRequest) -> anyhow::Result<SyncerSyncResponse>;
    async fn confirm(&self, request: SyncerConfirmRequest)
    -> anyhow::Result<SyncerConfirmResponse>;
}

#[derive(Clone)]
pub struct HttpControlPlaneClient {
    base_url: String,
    syncer_key: String,
    client: reqwest::Client,
}

impl HttpControlPlaneClient {
    pub fn new(
        base_url: impl Into<String>,
        syncer_key: impl Into<String>,
        timeout: Duration,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            syncer_key: syncer_key.into(),
            client,
        })
    }

    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    pub fn syncer_key(&self) -> &str {
        &self.syncer_key
    }
}

#[async_trait]
impl ControlPlaneClient for HttpControlPlaneClient {
    async fn sync(&self, request: SyncerSyncRequest) -> anyhow::Result<SyncerSyncResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/syncer/sync"))
            .header("x-adscope-syncer-key", &self.syncer_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn confirm(
        &self,
        request: SyncerConfirmRequest,
    ) -> anyhow::Result<SyncerConfirmResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/syncer/confirm"))
            .header("x-adscope-syncer-key", &self.syncer_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
