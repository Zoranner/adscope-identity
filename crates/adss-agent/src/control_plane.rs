use adss_contract::{
    AgentConfirmRequest, AgentConfirmResponse, AgentSyncRequest, AgentSyncResponse,
};
use async_trait::async_trait;

#[async_trait]
pub trait ControlPlaneClient {
    async fn sync(&self, request: AgentSyncRequest) -> anyhow::Result<AgentSyncResponse>;
    async fn confirm(&self, request: AgentConfirmRequest) -> anyhow::Result<AgentConfirmResponse>;
}

#[derive(Clone)]
pub struct HttpControlPlaneClient {
    base_url: String,
    agent_key: String,
    client: reqwest::Client,
}

impl HttpControlPlaneClient {
    pub fn new(base_url: impl Into<String>, agent_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            agent_key: agent_key.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    pub fn agent_key(&self) -> &str {
        &self.agent_key
    }
}

#[async_trait]
impl ControlPlaneClient for HttpControlPlaneClient {
    async fn sync(&self, request: AgentSyncRequest) -> anyhow::Result<AgentSyncResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/agent/sync"))
            .header("x-adss-agent-key", &self.agent_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn confirm(&self, request: AgentConfirmRequest) -> anyhow::Result<AgentConfirmResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/agent/confirm"))
            .header("x-adss-agent-key", &self.agent_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
