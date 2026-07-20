use adss_contract::{
    AgentConfirmRequest, AgentConfirmResponse, AgentSyncRequest, AgentSyncResponse,
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DirectoryOperationKind,
    DirectoryOperationTarget, DirectoryPlan, DomainDirectoryConfig, Group, OrganizationalUnit,
    SyncChannel, SyncSummary, User, UserStatus,
};
use async_trait::async_trait;
use ldap3::{Ldap, LdapConnAsync, LdapConnSettings, Mod, Scope, SearchEntry};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[async_trait]
pub trait DirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;
    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()>;
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
    pub ldap: Option<LdapDirectoryConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapDirectoryConfig {
    pub url: String,
    pub bind_dn: String,
    pub bind_password: String,
    pub accept_invalid_certs: bool,
}

impl AgentProcessConfig {
    pub fn new(
        server_url: impl Into<String>,
        domain_id: impl Into<String>,
        agent_key: impl Into<String>,
        state_path: impl Into<String>,
        interval_seconds: u64,
        dry_run: bool,
        ldap: Option<LdapDirectoryConfig>,
    ) -> Self {
        Self {
            server_url: server_url.into(),
            domain_id: domain_id.into(),
            agent_key: agent_key.into(),
            state_path: state_path.into(),
            interval_seconds,
            dry_run,
            ldap,
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
        let ldap = if dry_run {
            None
        } else {
            let url = required_env("ADSS_LDAP_URL")?;
            if !url
                .get(..8)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("ldaps://"))
            {
                anyhow::bail!("ADSS_LDAP_URL must use ldaps://");
            }
            Some(LdapDirectoryConfig {
                url,
                bind_dn: required_env("ADSS_LDAP_BIND_DN")?,
                bind_password: required_env("ADSS_LDAP_BIND_PASSWORD")?,
                accept_invalid_certs: std::env::var("ADSS_LDAP_ACCEPT_INVALID_CERTS")
                    .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                    .unwrap_or(false),
            })
        };

        Ok(Self::new(
            server_url,
            domain_id,
            agent_key,
            state_path,
            interval_seconds,
            dry_run,
            ldap,
        ))
    }
}

fn required_env(name: &'static str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required"))
}

fn validate_ldap_attribute_name(name: &str) -> anyhow::Result<()> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        anyhow::bail!("LDAP attribute name contains unsupported characters: {name}")
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

pub enum ConfiguredDirectoryClient {
    DryRun(DryRunDirectoryClient),
    Ldap(LdapDirectoryClient),
}

impl ConfiguredDirectoryClient {
    pub fn from_process_config(config: &AgentProcessConfig) -> anyhow::Result<Self> {
        if config.dry_run {
            return Ok(Self::DryRun(DryRunDirectoryClient));
        }

        let ldap = config
            .ldap
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LDAP settings are required without dry-run"))?;
        Ok(Self::Ldap(LdapDirectoryClient::new(ldap)))
    }
}

#[async_trait]
impl DirectoryClient for ConfiguredDirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(client) => client.apply(operation, context).await,
            Self::Ldap(client) => client.apply(operation, context).await,
        }
    }

    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        match self {
            Self::DryRun(client) => client.set_password(credential, context).await,
            Self::Ldap(client) => client.set_password(credential, context).await,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryExecutionContext {
    pub domain: DomainDirectoryConfig,
    organizational_unit_dns: BTreeMap<String, String>,
}

impl DirectoryExecutionContext {
    pub fn from_domain(domain: &DomainDirectoryConfig) -> Self {
        Self {
            domain: domain.clone(),
            organizational_unit_dns: BTreeMap::new(),
        }
    }

    pub fn try_from_batch(
        batch: &DirectoryBatch,
        domain: &DomainDirectoryConfig,
    ) -> anyhow::Result<Self> {
        let mut by_id = BTreeMap::new();
        for ou in &batch.organizational_units {
            by_id.insert(ou.id.as_str(), ou);
        }

        let mut organizational_unit_dns = BTreeMap::new();
        for ou in &batch.organizational_units {
            build_organizational_unit_dn(ou, &by_id, &mut organizational_unit_dns, domain)?;
        }

        Ok(Self {
            domain: domain.clone(),
            organizational_unit_dns,
        })
    }

    pub fn organizational_unit_dn(&self, organizational_unit_id: &str) -> anyhow::Result<&str> {
        self.organizational_unit_dns
            .get(organizational_unit_id)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing OU DN for {organizational_unit_id}"))
    }
}

fn build_organizational_unit_dn<'a>(
    ou: &'a OrganizationalUnit,
    by_id: &BTreeMap<&str, &'a OrganizationalUnit>,
    organizational_unit_dns: &mut BTreeMap<String, String>,
    domain: &DomainDirectoryConfig,
) -> anyhow::Result<String> {
    if let Some(dn) = organizational_unit_dns.get(&ou.id) {
        return Ok(dn.clone());
    }

    let parent_dn = match &ou.parent_id {
        Some(parent_id) => {
            let parent = by_id
                .get(parent_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing parent OU {parent_id} for {}", ou.id))?;
            build_organizational_unit_dn(parent, by_id, organizational_unit_dns, domain)?
        }
        None => domain.mirror_root_dn.clone(),
    };
    let dn = format!("OU={},{}", escape_ldap_dn_value(&ou.name), parent_dn);
    organizational_unit_dns.insert(ou.id.clone(), dn.clone());
    Ok(dn)
}

#[derive(Debug, Clone)]
pub struct LdapDirectoryClient {
    config: LdapDirectoryConfig,
}

impl LdapDirectoryClient {
    pub fn new(config: LdapDirectoryConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LdapDirectoryConfig {
        &self.config
    }

    async fn bind(&self) -> anyhow::Result<Ldap> {
        let settings = LdapConnSettings::new().set_no_tls_verify(self.config.accept_invalid_certs);
        let (connection, mut ldap) =
            LdapConnAsync::with_settings(settings, &self.config.url).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.drive().await {
                eprintln!("ldap connection failed: {error}");
            }
        });
        ldap.simple_bind(&self.config.bind_dn, &self.config.bind_password)
            .await?
            .success()?;
        Ok(ldap)
    }

    async fn ensure_ou(
        &self,
        ldap: &mut Ldap,
        ou: &OrganizationalUnit,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = context.organizational_unit_dn(&ou.id)?;
        if search_base_exists(ldap, dn, "(objectClass=organizationalUnit)").await? {
            ldap.modify(
                dn,
                vec![
                    replace_string("ou", &ou.name),
                    replace_string("name", &ou.name),
                ],
            )
            .await?
            .success()?;
            return Ok(());
        }

        ldap.add(
            dn,
            vec![
                ldap_attr("objectClass", &["top", "organizationalUnit"]),
                ldap_attr("ou", &[&ou.name]),
                ldap_attr("name", &[&ou.name]),
            ],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_user(
        &self,
        ldap: &mut Ldap,
        user: &User,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let user_account_control = match user.status {
            UserStatus::Active => "512",
            UserStatus::Disabled => "514",
        };

        if let Some(dn) = find_user_dn(ldap, context, &user.employee_id).await? {
            ldap.modify(
                &dn,
                user_mods(
                    user,
                    &context.domain.employee_id_attribute,
                    &context.domain.upn_suffix,
                    user_account_control,
                ),
            )
            .await?
            .success()?;
            return Ok(());
        }

        let ou_dn = context.organizational_unit_dn(&user.organizational_unit_id)?;
        let dn = format!("CN={},{}", escape_ldap_dn_value(&user.username), ou_dn);
        ldap.add(
            &dn,
            user_attrs(
                user,
                &context.domain.employee_id_attribute,
                &context.domain.upn_suffix,
                "514",
            ),
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_user_placement(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        organizational_unit_id: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        let target_parent_dn = context.organizational_unit_dn(organizational_unit_id)?;
        if dn_is_under_parent(&dn, target_parent_dn) {
            return Ok(());
        }
        let rdn = dn_rdn(&dn)?;
        ldap.modifydn(&dn, rdn, true, Some(target_parent_dn))
            .await?
            .success()?;
        Ok(())
    }

    async fn ensure_group(
        &self,
        ldap: &mut Ldap,
        group: &Group,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        if let Some(dn) = find_group_dn(ldap, context, &group.id).await? {
            ldap.modify(
                &dn,
                vec![
                    replace_string("sAMAccountName", &group.name),
                    replace_string("description", &group_marker(&group.id)),
                ],
            )
            .await?
            .success()?;
            return Ok(());
        }

        let dn = format!(
            "CN={},{}",
            escape_ldap_dn_value(&group.name),
            context.domain.mirror_root_dn
        );
        ldap.add(
            &dn,
            vec![
                ldap_attr("objectClass", &["top", "group"]),
                ldap_attr("cn", &[&group.name]),
                ldap_attr("sAMAccountName", &[&group.name]),
                ldap_attr("groupType", &["-2147483646"]),
                ldap_attr("description", &[&group_marker(&group.id)]),
            ],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn ensure_group_members(
        &self,
        ldap: &mut Ldap,
        group: &Group,
        member_employee_ids: &[String],
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let group_dn = require_group_dn(ldap, context, &group.id).await?;
        let mut member_dns = HashSet::new();
        for employee_id in member_employee_ids {
            member_dns.insert(
                require_user_dn(ldap, context, employee_id)
                    .await?
                    .into_bytes(),
            );
        }
        ldap.modify(
            &group_dn,
            vec![Mod::Replace(b"member".to_vec(), member_dns)],
        )
        .await?
        .success()?;
        Ok(())
    }

    async fn disable_user(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        ldap.modify(&dn, vec![replace_string("userAccountControl", "514")])
            .await?
            .success()?;
        Ok(())
    }

    async fn move_user_to_quarantine(
        &self,
        ldap: &mut Ldap,
        employee_id: &str,
        quarantine_dn: &str,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let dn = require_user_dn(ldap, context, employee_id).await?;
        if dn_is_under_parent(&dn, quarantine_dn) {
            return Ok(());
        }
        let rdn = dn_rdn(&dn)?;
        ldap.modifydn(&dn, rdn, true, Some(quarantine_dn))
            .await?
            .success()?;
        Ok(())
    }
}

#[async_trait]
impl DirectoryClient for LdapDirectoryClient {
    async fn apply(
        &self,
        operation: &DirectoryOperation,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let mut ldap = self.bind().await?;
        match (operation.kind, operation.target.as_ref()) {
            (
                DirectoryOperationKind::EnsureOu,
                Some(DirectoryOperationTarget::OrganizationalUnit(ou)),
            ) => self.ensure_ou(&mut ldap, ou, context).await,
            (DirectoryOperationKind::EnsureUser, Some(DirectoryOperationTarget::User(user))) => {
                self.ensure_user(&mut ldap, user, context).await
            }
            (
                DirectoryOperationKind::EnsureUserPlacement,
                Some(DirectoryOperationTarget::UserOrganizationalUnitId(ou_id)),
            ) => {
                self.ensure_user_placement(&mut ldap, &operation.subject, ou_id, context)
                    .await
            }
            (DirectoryOperationKind::EnsureGroup, Some(DirectoryOperationTarget::Group(group))) => {
                self.ensure_group(&mut ldap, group, context).await
            }
            (
                DirectoryOperationKind::EnsureGroupMembers,
                Some(DirectoryOperationTarget::GroupMembers {
                    group,
                    member_employee_ids,
                }),
            ) => {
                self.ensure_group_members(&mut ldap, group, member_employee_ids, context)
                    .await
            }
            (DirectoryOperationKind::DisableUser, None) => {
                self.disable_user(&mut ldap, &operation.subject, context)
                    .await
            }
            (
                DirectoryOperationKind::MoveUserToQuarantine,
                Some(DirectoryOperationTarget::QuarantineDn(quarantine_dn)),
            ) => {
                self.move_user_to_quarantine(&mut ldap, &operation.subject, quarantine_dn, context)
                    .await
            }
            _ => anyhow::bail!("directory operation target does not match operation kind"),
        }
    }

    async fn set_password(
        &self,
        credential: &CredentialEntry,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        let mut ldap = self.bind().await?;
        let dn = require_user_dn(&mut ldap, context, &credential.employee_id).await?;
        ldap.modify(
            &dn,
            vec![Mod::Replace(
                b"unicodePwd".to_vec(),
                HashSet::from([encode_ad_unicode_password(&credential.plaintext_password)]),
            )],
        )
        .await?
        .success()?;
        let user_account_control = match credential.status {
            UserStatus::Active => "512",
            UserStatus::Disabled => "514",
        };
        ldap.modify(
            &dn,
            vec![replace_string("userAccountControl", user_account_control)],
        )
        .await?
        .success()?;
        Ok(())
    }
}

async fn search_base_exists(ldap: &mut Ldap, dn: &str, filter: &str) -> anyhow::Result<bool> {
    let (entries, _) = ldap
        .search(dn, Scope::Base, filter, vec!["distinguishedName"])
        .await?
        .success()?;
    Ok(!entries.is_empty())
}

async fn find_user_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    employee_id: &str,
) -> anyhow::Result<Option<String>> {
    validate_ldap_attribute_name(&context.domain.employee_id_attribute)?;
    let filter = format!(
        "(&(objectClass=user)({}={}))",
        context.domain.employee_id_attribute,
        escape_ldap_filter_value(employee_id)
    );
    search_managed_dn(ldap, context, &filter).await
}

async fn require_user_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    employee_id: &str,
) -> anyhow::Result<String> {
    find_user_dn(ldap, context, employee_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("managed AD user not found for employee {employee_id}"))
}

async fn find_group_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    group_id: &str,
) -> anyhow::Result<Option<String>> {
    let filter = format!(
        "(&(objectClass=group)(description={}))",
        escape_ldap_filter_value(&group_marker(group_id))
    );
    search_managed_dn(ldap, context, &filter).await
}

async fn require_group_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    group_id: &str,
) -> anyhow::Result<String> {
    find_group_dn(ldap, context, group_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("managed AD group not found: {group_id}"))
}

async fn search_managed_dn(
    ldap: &mut Ldap,
    context: &DirectoryExecutionContext,
    filter: &str,
) -> anyhow::Result<Option<String>> {
    let mut matched_dn = None;
    for base in managed_search_bases(context) {
        let (entries, _) = ldap
            .search(&base, Scope::Subtree, filter, vec!["distinguishedName"])
            .await?
            .success()?;
        for entry in entries {
            let dn = SearchEntry::construct(entry).dn;
            if matched_dn.is_some() {
                anyhow::bail!("managed AD search returned more than one object for {filter}");
            }
            matched_dn = Some(dn);
        }
    }
    Ok(matched_dn)
}

fn managed_search_bases(context: &DirectoryExecutionContext) -> Vec<String> {
    if context.domain.quarantine_ou_dn == context.domain.mirror_root_dn {
        vec![context.domain.mirror_root_dn.clone()]
    } else {
        vec![
            context.domain.mirror_root_dn.clone(),
            context.domain.quarantine_ou_dn.clone(),
        ]
    }
}

fn group_marker(group_id: &str) -> String {
    format!("adss:group_id:{group_id}")
}

fn user_attrs(
    user: &User,
    employee_id_attribute: &str,
    upn_suffix: &str,
    user_account_control: &str,
) -> Vec<(Vec<u8>, HashSet<Vec<u8>>)> {
    let mut attrs = vec![
        ldap_attr(
            "objectClass",
            &["top", "person", "organizationalPerson", "user"],
        ),
        ldap_attr("cn", &[&user.username]),
        ldap_attr("sn", &[&user.display_name]),
        ldap_attr("displayName", &[&user.display_name]),
        ldap_attr("sAMAccountName", &[&user.username]),
        ldap_attr(
            "userPrincipalName",
            &[&format!("{}@{}", user.username, upn_suffix)],
        ),
        ldap_attr(employee_id_attribute, &[&user.employee_id]),
        ldap_attr("userAccountControl", &[user_account_control]),
    ];
    push_optional_attr(&mut attrs, "mail", user.email.as_deref());
    push_optional_attr(&mut attrs, "mobile", user.mobile.as_deref());
    push_optional_attr(&mut attrs, "telephoneNumber", user.telephone.as_deref());
    attrs
}

fn user_mods(
    user: &User,
    employee_id_attribute: &str,
    upn_suffix: &str,
    user_account_control: &str,
) -> Vec<Mod<Vec<u8>>> {
    vec![
        replace_string("sn", &user.display_name),
        replace_string("displayName", &user.display_name),
        replace_string("sAMAccountName", &user.username),
        replace_string(
            "userPrincipalName",
            &format!("{}@{}", user.username, upn_suffix),
        ),
        replace_string(employee_id_attribute, &user.employee_id),
        replace_string("userAccountControl", user_account_control),
        replace_optional("mail", user.email.as_deref()),
        replace_optional("mobile", user.mobile.as_deref()),
        replace_optional("telephoneNumber", user.telephone.as_deref()),
    ]
}

fn ldap_attr(name: &str, values: &[&str]) -> (Vec<u8>, HashSet<Vec<u8>>) {
    (
        name.as_bytes().to_vec(),
        values
            .iter()
            .map(|value| value.as_bytes().to_vec())
            .collect(),
    )
}

fn push_optional_attr(
    attrs: &mut Vec<(Vec<u8>, HashSet<Vec<u8>>)>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        attrs.push(ldap_attr(name, &[value]));
    }
}

fn replace_string(name: &str, value: &str) -> Mod<Vec<u8>> {
    Mod::Replace(
        name.as_bytes().to_vec(),
        HashSet::from([value.as_bytes().to_vec()]),
    )
}

fn replace_optional(name: &str, value: Option<&str>) -> Mod<Vec<u8>> {
    Mod::Replace(
        name.as_bytes().to_vec(),
        value
            .map(|value| HashSet::from([value.as_bytes().to_vec()]))
            .unwrap_or_default(),
    )
}

fn dn_is_under_parent(dn: &str, parent_dn: &str) -> bool {
    dn.len() > parent_dn.len()
        && dn.ends_with(parent_dn)
        && dn.as_bytes().get(dn.len() - parent_dn.len() - 1).copied() == Some(b',')
}

fn dn_rdn(dn: &str) -> anyhow::Result<&str> {
    let mut escaped = false;
    for (index, character) in dn.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == ',' {
            return Ok(&dn[..index]);
        }
    }
    anyhow::bail!("DN has no parent component: {dn}")
}

pub fn escape_ldap_filter_value(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '*' => escaped.push_str(r"\2a"),
            '(' => escaped.push_str(r"\28"),
            ')' => escaped.push_str(r"\29"),
            '\\' => escaped.push_str(r"\5c"),
            '\0' => escaped.push_str(r"\00"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub fn escape_ldap_dn_value(value: &str) -> String {
    let mut escaped = String::new();
    let mut chars = value.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        let is_first = index == 0;
        let is_last = chars.peek().is_none();
        if (is_first && (character == ' ' || character == '#'))
            || (is_last && character == ' ')
            || matches!(character, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=')
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }

    escaped
}

pub fn encode_ad_unicode_password(password: &str) -> Vec<u8> {
    format!("\"{password}\"")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
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

        let credential_context = DirectoryExecutionContext::from_domain(&response.directory_config);
        let credential_summary =
            execute_credential_batch(&self.directory, &response.credentials, &credential_context)
                .await;
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
    pub async fn execute(
        &self,
        plan: &DirectoryPlan,
        context: &DirectoryExecutionContext,
    ) -> anyhow::Result<SyncSummary> {
        Ok(execute_directory_plan(&self.client, plan, context).await)
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
    let context = match DirectoryExecutionContext::try_from_batch(batch, domain) {
        Ok(context) => context,
        Err(_) => {
            return ChannelExecutionResult {
                summary: SyncSummary {
                    failed: 1,
                    ..SyncSummary::default()
                },
                succeeded: false,
                error_code: Some("directory_context_failed"),
            };
        }
    };
    let summary = execute_directory_plan(client, &plan, &context).await;
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

pub async fn execute_directory_plan<C>(
    client: &C,
    plan: &DirectoryPlan,
    context: &DirectoryExecutionContext,
) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, operation) in plan.operations.iter().enumerate() {
        match client.apply(operation, context).await {
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

pub async fn execute_credential_batch<C>(
    client: &C,
    batch: &CredentialBatch,
    context: &DirectoryExecutionContext,
) -> SyncSummary
where
    C: DirectoryClient + Sync,
{
    let mut summary = SyncSummary::default();

    for (index, credential) in batch.credentials.iter().enumerate() {
        match client.set_password(credential, context).await {
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
