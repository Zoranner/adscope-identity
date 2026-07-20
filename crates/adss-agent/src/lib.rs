use adss_contract::{
    AgentConfirmRequest, AgentConfirmResponse, AgentSyncRequest, AgentSyncResponse,
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DirectoryPlan,
    SyncChannel, SyncSummary,
};
use async_trait::async_trait;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[async_trait]
pub trait DirectoryClient {
    async fn apply(&self, operation: &DirectoryOperation) -> anyhow::Result<()>;
    async fn set_password(&self, credential: &CredentialEntry) -> anyhow::Result<()>;
}

#[async_trait]
pub trait ControlPlaneClient {
    async fn sync(&self, request: AgentSyncRequest) -> anyhow::Result<AgentSyncResponse>;
    async fn confirm(&self, request: AgentConfirmRequest) -> anyhow::Result<AgentConfirmResponse>;
}

pub trait LocalStateStore {
    fn load(&self) -> anyhow::Result<LocalRevisionState>;
    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()>;

    fn load_for_sync(&self) -> anyhow::Result<LocalStateLoad> {
        Ok(LocalStateLoad {
            state: self.load()?,
            rebuild_directory: false,
            rebuild_credentials: false,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalRevisionState {
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalStateLoad {
    pub state: LocalRevisionState,
    pub rebuild_directory: bool,
    pub rebuild_credentials: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessConfig {
    pub server_url: String,
    pub domain_id: String,
    pub agent_key: String,
    pub state_path: String,
    pub interval_seconds: u64,
    pub dry_run: bool,
}

impl AgentProcessConfig {
    pub fn new(
        server_url: impl Into<String>,
        domain_id: impl Into<String>,
        agent_key: impl Into<String>,
        state_path: impl Into<String>,
        interval_seconds: u64,
        dry_run: bool,
    ) -> Self {
        Self {
            server_url: server_url.into(),
            domain_id: domain_id.into(),
            agent_key: agent_key.into(),
            state_path: state_path.into(),
            interval_seconds,
            dry_run,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let server_url = std::env::var("ADSS_SERVER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let domain_id = std::env::var("ADSS_DOMAIN_ID")?;
        let agent_key = std::env::var("ADSS_AGENT_KEY")?;
        let state_path = std::env::var("ADSS_AGENT_STATE_PATH")?;
        let interval_seconds = std::env::var("ADSS_AGENT_INTERVAL_SECONDS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(60);
        if interval_seconds == 0 {
            anyhow::bail!("ADSS_AGENT_INTERVAL_SECONDS must be greater than 0");
        }
        let dry_run = std::env::var("ADSS_AGENT_DRY_RUN")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Ok(Self::new(
            server_url,
            domain_id,
            agent_key,
            state_path,
            interval_seconds,
            dry_run,
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

#[derive(Debug, Clone)]
pub struct FileLocalStateStore {
    path: PathBuf,
}

impl FileLocalStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalStateStore for FileLocalStateStore {
    fn load(&self) -> anyhow::Result<LocalRevisionState> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => parse_local_revision_state(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LocalRevisionState::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(format_local_revision_state(state).as_bytes())?;
            file.sync_all()?;
        }
        replace_file(&temp_path, &self.path)?;

        Ok(())
    }

    fn load_for_sync(&self) -> anyhow::Result<LocalStateLoad> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => match parse_local_revision_state(&contents) {
                Ok(state) => Ok(LocalStateLoad {
                    state,
                    rebuild_directory: false,
                    rebuild_credentials: false,
                }),
                Err(_) => Ok(LocalStateLoad {
                    state: LocalRevisionState::default(),
                    rebuild_directory: true,
                    rebuild_credentials: true,
                }),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LocalStateLoad::default())
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub struct DryRunDirectoryClient;

#[async_trait]
impl DirectoryClient for DryRunDirectoryClient {
    async fn apply(&self, _operation: &DirectoryOperation) -> anyhow::Result<()> {
        Ok(())
    }

    async fn set_password(&self, _credential: &CredentialEntry) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct AgentRuntime<P, D, S> {
    domain_id: String,
    control_plane: P,
    directory: D,
    local_state: S,
}

impl<P, D, S> AgentRuntime<P, D, S> {
    pub fn new(
        domain_id: impl Into<String>,
        control_plane: P,
        directory: D,
        local_state: S,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            control_plane,
            directory,
            local_state,
        }
    }

    pub fn control_plane(&self) -> &P {
        &self.control_plane
    }

    pub fn local_state_store(&self) -> &S {
        &self.local_state
    }
}

impl<P, D, S> AgentRuntime<P, D, S>
where
    S: LocalStateStore,
{
    pub fn local_state(&self) -> LocalRevisionState {
        self.local_state
            .load()
            .expect("local state should be readable")
    }
}

impl<P, D, S> AgentRuntime<P, D, S>
where
    P: ControlPlaneClient + Sync,
    D: DirectoryClient + Sync,
    S: LocalStateStore + Sync,
{
    pub async fn run_once(&mut self) -> anyhow::Result<AgentRunSummary> {
        let local_state_load = self.local_state.load_for_sync()?;
        let mut local_state = local_state_load.state;
        let response = self
            .control_plane
            .sync(AgentSyncRequest {
                domain_id: self.domain_id.clone(),
                applied_directory_revision: local_state.applied_directory_revision,
                applied_credential_revision: local_state.applied_credential_revision,
                rebuild_directory: local_state_load.rebuild_directory,
                rebuild_credentials: local_state_load.rebuild_credentials,
            })
            .await?;

        let mut run_summary = AgentRunSummary::default();
        let mut first_error = None;

        let directory_result = execute_directory_batch(
            &self.directory,
            &response.directory,
            &response.directory_config,
        )
        .await;
        run_summary.directory = directory_result.summary;
        if directory_result.succeeded {
            match self
                .confirm_channel(
                    SyncChannel::Directory,
                    response.directory.confirm_revision(),
                    &mut local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        } else if let Some(error_code) = directory_result.error_code {
            match self
                .confirm_failed_channel(
                    SyncChannel::Directory,
                    response.directory.confirm_revision(),
                    error_code,
                    local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        let credential_summary =
            execute_credential_batch(&self.directory, &response.credentials).await;
        let credential_succeeded =
            !response.credentials.credentials.is_empty() && credential_summary.failed == 0;
        let credential_error_code = if credential_summary.failed > 0 {
            Some("credential_execution_failed")
        } else {
            None
        };
        run_summary.credentials = credential_summary;
        if credential_succeeded {
            match self
                .confirm_channel(
                    SyncChannel::Credential,
                    response.credentials.confirm_revision(),
                    &mut local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        } else if let Some(error_code) = credential_error_code {
            match self
                .confirm_failed_channel(
                    SyncChannel::Credential,
                    response.credentials.confirm_revision(),
                    error_code,
                    local_state,
                )
                .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(run_summary)
        }
    }

    async fn confirm_channel(
        &self,
        channel: SyncChannel,
        revision: u64,
        local_state: &mut LocalRevisionState,
    ) -> anyhow::Result<()> {
        if !should_confirm(channel, revision, *local_state) {
            return Ok(());
        }

        let response = self
            .control_plane
            .confirm(AgentConfirmRequest {
                domain_id: self.domain_id.clone(),
                channel,
                target_revision: revision,
                success: true,
                error_code: None,
            })
            .await?;

        if response.accepted {
            match channel {
                SyncChannel::Directory => {
                    local_state.applied_directory_revision = revision;
                }
                SyncChannel::Credential => {
                    local_state.applied_credential_revision = revision;
                }
            }
            self.local_state.save(*local_state)?;
        }

        Ok(())
    }

    async fn confirm_failed_channel(
        &self,
        channel: SyncChannel,
        revision: u64,
        error_code: &'static str,
        local_state: LocalRevisionState,
    ) -> anyhow::Result<()> {
        if !should_confirm(channel, revision, local_state) {
            return Ok(());
        }

        self.control_plane
            .confirm(AgentConfirmRequest {
                domain_id: self.domain_id.clone(),
                channel,
                target_revision: revision,
                success: false,
                error_code: Some(error_code.to_string()),
            })
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRunSummary {
    pub directory: SyncSummary,
    pub credentials: SyncSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelExecutionResult {
    summary: SyncSummary,
    succeeded: bool,
    error_code: Option<&'static str>,
}

pub struct DirectoryExecutor<C> {
    client: C,
}

impl<C> DirectoryExecutor<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> DirectoryExecutor<C>
where
    C: DirectoryClient + Sync,
{
    pub async fn execute(&self, plan: &DirectoryPlan) -> anyhow::Result<SyncSummary> {
        Ok(execute_directory_plan(&self.client, plan).await)
    }
}

async fn execute_directory_batch<C>(
    client: &C,
    batch: &DirectoryBatch,
    domain: &adss_contract::DomainDirectoryConfig,
) -> ChannelExecutionResult
where
    C: DirectoryClient + Sync,
{
    if batch.organizational_units.is_empty() && batch.users.is_empty() && batch.groups.is_empty() {
        return ChannelExecutionResult {
            summary: SyncSummary::default(),
            succeeded: false,
            error_code: None,
        };
    }

    let plan = match DirectoryPlan::try_from_batch(batch, domain) {
        Ok(plan) => plan,
        Err(_) => {
            return ChannelExecutionResult {
                summary: SyncSummary {
                    failed: 1,
                    ..SyncSummary::default()
                },
                succeeded: false,
                error_code: Some("directory_plan_failed"),
            };
        }
    };
    let summary = execute_directory_plan(client, &plan).await;
    let succeeded = summary.failed == 0;
    let error_code = if succeeded {
        None
    } else {
        Some("directory_execution_failed")
    };

    ChannelExecutionResult {
        summary,
        succeeded,
        error_code,
    }
}

pub async fn execute_directory_plan<C>(client: &C, plan: &DirectoryPlan) -> SyncSummary
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

pub async fn execute_credential_batch<C>(client: &C, batch: &CredentialBatch) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, credential) in batch.credentials.iter().enumerate() {
        match client.set_password(credential).await {
            Ok(()) => summary.succeeded += 1,
            Err(_) => {
                summary.failed += 1;
                summary.skipped += (batch.credentials.len() - index - 1) as u32;
                break;
            }
        }
    }

    summary
}

fn should_confirm(channel: SyncChannel, revision: u64, local_state: LocalRevisionState) -> bool {
    match channel {
        SyncChannel::Directory => revision > local_state.applied_directory_revision,
        SyncChannel::Credential => revision > local_state.applied_credential_revision,
    }
}

fn parse_local_revision_state(contents: &str) -> anyhow::Result<LocalRevisionState> {
    let contents = contents.trim();
    let Some(body) = contents
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        anyhow::bail!("invalid local state JSON object");
    };

    let mut applied_directory_revision = None;
    let mut applied_credential_revision = None;

    for field in body
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
    {
        let Some((key, value)) = field.split_once(':') else {
            anyhow::bail!("invalid local state field");
        };
        let key = key.trim().trim_matches('"');
        let value = value.trim().parse::<u64>()?;

        match key {
            "applied_directory_revision" => applied_directory_revision = Some(value),
            "applied_credential_revision" => applied_credential_revision = Some(value),
            _ => anyhow::bail!("unknown local state field: {key}"),
        }
    }

    Ok(LocalRevisionState {
        applied_directory_revision: applied_directory_revision
            .ok_or_else(|| anyhow::anyhow!("missing applied_directory_revision"))?,
        applied_credential_revision: applied_credential_revision
            .ok_or_else(|| anyhow::anyhow!("missing applied_credential_revision"))?,
    })
}

fn format_local_revision_state(state: LocalRevisionState) -> String {
    format!(
        "{{\"applied_directory_revision\":{},\"applied_credential_revision\":{}}}",
        state.applied_directory_revision, state.applied_credential_revision
    )
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}
