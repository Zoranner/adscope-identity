use adss_contract::{
    AdOperation, AgentPollRequest, AgentReportRequest, AgentReportResponse, DomainConfig,
    PasswordTask, PollStructurePayload, ReconcilePlan, SyncSummary,
};
use async_trait::async_trait;

#[async_trait]
pub trait DirectoryClient {
    async fn apply(&self, operation: &AdOperation) -> anyhow::Result<()>;

    async fn set_password(&self, _task: &PasswordTask) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait ControlPlaneClient {
    async fn poll(
        &self,
        request: AgentPollRequest,
    ) -> anyhow::Result<adss_contract::AgentPollResponse>;

    async fn report(&self, request: AgentReportRequest) -> anyhow::Result<AgentReportResponse>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessConfig {
    pub server_url: String,
    pub domain_id: String,
    pub agent_id: String,
    pub agent_key: String,
    pub dry_run: bool,
    pub initial_structure_version: u64,
    pub initial_password_task_cursor: u64,
}

impl AgentProcessConfig {
    pub fn new(
        server_url: impl Into<String>,
        domain_id: impl Into<String>,
        agent_id: impl Into<String>,
        agent_key: impl Into<String>,
        dry_run: bool,
    ) -> Self {
        Self {
            server_url: server_url.into(),
            domain_id: domain_id.into(),
            agent_id: agent_id.into(),
            agent_key: agent_key.into(),
            dry_run,
            initial_structure_version: 0,
            initial_password_task_cursor: 0,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let server_url = std::env::var("ADSS_SERVER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let domain_id = std::env::var("ADSS_DOMAIN_ID")?;
        let agent_id = std::env::var("ADSS_AGENT_ID")?;
        let agent_key = std::env::var("ADSS_AGENT_KEY")?;
        let dry_run = std::env::var("ADSS_AGENT_DRY_RUN")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Ok(Self::new(
            server_url, domain_id, agent_id, agent_key, dry_run,
        ))
    }
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
    async fn poll(
        &self,
        request: AgentPollRequest,
    ) -> anyhow::Result<adss_contract::AgentPollResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/agent/poll"))
            .header("x-adss-agent-key", &self.agent_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn report(&self, request: AgentReportRequest) -> anyhow::Result<AgentReportResponse> {
        Ok(self
            .client
            .post(self.endpoint("/api/agent/report"))
            .header("x-adss-agent-key", &self.agent_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCursor {
    pub structure_version: u64,
    pub password_task_cursor: u64,
}

pub struct DryRunDirectoryClient;

#[async_trait]
impl DirectoryClient for DryRunDirectoryClient {
    async fn apply(&self, _operation: &AdOperation) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_password(&self, _task: &PasswordTask) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct AgentRuntime<P, D> {
    domain_id: String,
    agent_id: String,
    domain: DomainConfig,
    cursor: AgentCursor,
    control_plane: P,
    directory: D,
}

impl<P, D> AgentRuntime<P, D> {
    pub fn new(
        domain_id: impl Into<String>,
        agent_id: impl Into<String>,
        domain: DomainConfig,
        cursor: AgentCursor,
        control_plane: P,
        directory: D,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            agent_id: agent_id.into(),
            domain,
            cursor,
            control_plane,
            directory,
        }
    }

    pub fn cursor(&self) -> AgentCursor {
        self.cursor
    }
}

impl<P, D> AgentRuntime<P, D>
where
    P: ControlPlaneClient + Sync,
    D: DirectoryClient + Sync,
{
    pub async fn run_once(&mut self) -> anyhow::Result<SyncSummary> {
        let poll = self
            .control_plane
            .poll(AgentPollRequest {
                domain_id: self.domain_id.clone(),
                agent_id: self.agent_id.clone(),
                last_structure_version: self.cursor.structure_version,
                password_task_cursor: self.cursor.password_task_cursor,
            })
            .await?;

        let mut summary = SyncSummary::default();
        let applied_structure_version = match poll.structure {
            PollStructurePayload::NoChange => self.cursor.structure_version,
            PollStructurePayload::Delta(state) | PollStructurePayload::Snapshot(state) => {
                let plan = ReconcilePlan::from_desired_state(&state, &self.domain);
                let structure_summary = execute_reconcile_plan(&self.directory, &plan).await;
                merge_summary(&mut summary, structure_summary);
                state.version
            }
        };

        let mut applied_password_task_cursor = self.cursor.password_task_cursor;
        for task in &poll.password_tasks {
            match self.directory.set_password(task).await {
                Ok(()) => {
                    summary.succeeded += 1;
                    applied_password_task_cursor = applied_password_task_cursor.max(task.task_id);
                }
                Err(_) => {
                    summary.failed += 1;
                    break;
                }
            }
        }

        self.control_plane
            .report(AgentReportRequest {
                domain_id: self.domain_id.clone(),
                agent_id: self.agent_id.clone(),
                applied_structure_version,
                applied_password_task_cursor,
                summary: summary.clone(),
                object_results: Vec::new(),
            })
            .await?;

        self.cursor = AgentCursor {
            structure_version: applied_structure_version,
            password_task_cursor: applied_password_task_cursor,
        };

        Ok(summary)
    }
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

fn merge_summary(target: &mut SyncSummary, source: SyncSummary) {
    target.succeeded += source.succeeded;
    target.failed += source.failed;
    target.skipped += source.skipped;
    target.pending_manual += source.pending_manual;
}
