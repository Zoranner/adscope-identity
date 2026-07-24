use adss_protocol::{
    ConnectorConfirmRequest, ConnectorConfirmResponse, ConnectorSyncRequest, ConnectorSyncResponse,
};
use async_trait::async_trait;

#[async_trait]
pub trait ControlPlaneClient {
    async fn sync(&self, request: ConnectorSyncRequest) -> anyhow::Result<ConnectorSyncResponse>;
    async fn confirm(
        &self,
        request: ConnectorConfirmRequest,
    ) -> anyhow::Result<ConnectorConfirmResponse>;
}

#[derive(Clone)]
pub struct HttpControlPlaneClient {
    base_url: String,
    connector_key: String,
    client: reqwest::Client,
}

impl HttpControlPlaneClient {
    pub fn new(base_url: impl Into<String>, connector_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            connector_key: connector_key.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    pub fn connector_key(&self) -> &str {
        &self.connector_key
    }
}

#[async_trait]
impl ControlPlaneClient for HttpControlPlaneClient {
    async fn sync(&self, request: ConnectorSyncRequest) -> anyhow::Result<ConnectorSyncResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/connector/sync"))
            .header("x-adss-connector-key", &self.connector_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn confirm(
        &self,
        request: ConnectorConfirmRequest,
    ) -> anyhow::Result<ConnectorConfirmResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/connector/confirm"))
            .header("x-adss-connector-key", &self.connector_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
