use adss_contract::{
    AgentPollRequest, AgentPollResponse, AgentReportRequest, AgentReportResponse, AuditEvent,
    DesiredState, DomainConfig, DriftReportRequest, Group, OrgUnit, PasswordChangeRequest,
    PasswordChangeResponse, PasswordTask, PollStructurePayload, RegisterAgentRequest,
    RegisterAgentResponse, UpdateUserRequest, User, UserStatus, sanitize_user_attributes,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub bind_addr: String,
}

impl ServerConfig {
    pub fn from_bind_addr(bind_addr: Option<String>) -> Self {
        Self {
            bind_addr: bind_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<Store>>,
}

impl AppState {
    pub fn seeded() -> Self {
        let desired_state = DesiredState {
            version: 3,
            ous: vec![OrgUnit {
                id: "ou-engineering".to_string(),
                relative_dn: "OU=Engineering".to_string(),
            }],
            groups: vec![Group {
                id: "group-engineering".to_string(),
                sam_account_name: "engineering".to_string(),
                relative_dn: "CN=engineering,OU=Engineering".to_string(),
            }],
            users: vec![User {
                employee_id: "E001".to_string(),
                sam_account_name: "zhangsan".to_string(),
                upn: "zhangsan@example.com".to_string(),
                display_name: "张三".to_string(),
                relative_dn: "CN=张三,OU=Engineering".to_string(),
                status: UserStatus::Active,
                attributes: BTreeMap::new(),
            }],
            memberships: Vec::new(),
        };

        let domains = HashMap::from([
            (
                "domain-a".to_string(),
                DomainConfig {
                    domain_id: "domain-a".to_string(),
                    mirror_root_dn: "OU=Mirror,DC=a,DC=example,DC=com".to_string(),
                    quarantine_ou_dn: "OU=Quarantine,DC=a,DC=example,DC=com".to_string(),
                    employee_id_attribute: "employeeID".to_string(),
                },
            ),
            (
                "domain-b".to_string(),
                DomainConfig {
                    domain_id: "domain-b".to_string(),
                    mirror_root_dn: "OU=Mirror,DC=b,DC=example,DC=com".to_string(),
                    quarantine_ou_dn: "OU=Quarantine,DC=b,DC=example,DC=com".to_string(),
                    employee_id_attribute: "employeeID".to_string(),
                },
            ),
        ]);

        let agents = HashMap::from([
            ("agent-a".to_string(), "domain-a".to_string()),
            ("agent-b".to_string(), "domain-b".to_string()),
        ]);
        let registration_tokens = HashMap::from([
            ("register-domain-a".to_string(), "domain-a".to_string()),
            ("register-domain-b".to_string(), "domain-b".to_string()),
        ]);

        Self {
            inner: Arc::new(Mutex::new(Store {
                desired_state,
                domains,
                agents,
                registration_tokens,
                drift_reports: Vec::new(),
                audit_events: Vec::new(),
                agent_structure_versions: HashMap::new(),
                password_tasks: Vec::new(),
                agent_cursors: HashMap::new(),
                next_password_task_id: 1,
                next_audit_sequence: 1,
            })),
        }
    }
}

#[derive(Debug)]
struct Store {
    desired_state: DesiredState,
    domains: HashMap<String, DomainConfig>,
    agents: HashMap<String, String>,
    registration_tokens: HashMap<String, String>,
    drift_reports: Vec<DriftReportRequest>,
    audit_events: Vec<AuditEvent>,
    agent_structure_versions: HashMap<String, u64>,
    password_tasks: Vec<PasswordTask>,
    agent_cursors: HashMap<String, u64>,
    next_password_task_id: u64,
    next_audit_sequence: u64,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/users/{employee_id}", patch(update_user))
        .route("/api/users/{employee_id}/password", post(change_password))
        .route("/api/org-tree", put(update_org_tree))
        .route("/api/sync/domains/{domain_id}/status", get(domain_status))
        .route("/api/audit/events", get(audit_events))
        .route("/api/agent/register", post(register_agent))
        .route("/api/agent/poll", post(agent_poll))
        .route("/api/agent/report", post(agent_report))
        .route("/api/agent/drift-report", post(drift_report))
        .with_state(state)
}

async fn login() -> Json<StatusMessage> {
    Json(StatusMessage::ok("local_auth_placeholder"))
}

async fn update_user(
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<User>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    let user = store
        .desired_state
        .users
        .iter_mut()
        .find(|user| user.employee_id == employee_id)
        .ok_or(ApiError::NotFound)?;

    if let Some(display_name) = request.display_name {
        user.display_name = display_name;
    }
    if let Some(relative_dn) = request.relative_dn {
        user.relative_dn = relative_dn;
    }
    user.attributes = sanitize_user_attributes(request.attributes, &["mail", "telephoneNumber"]);
    let updated_user = user.clone();
    store.desired_state.version += 1;
    let target = format!("user:{employee_id}");
    let version = store.desired_state.version.to_string();
    record_audit(
        &mut store,
        "user:local",
        "user_updated",
        target,
        "accepted",
        BTreeMap::from([("structure_version".to_string(), version)]),
    );

    Ok(Json(updated_user))
}

async fn change_password(
    State(state): State<AppState>,
    Path(employee_id): Path<String>,
    Json(request): Json<PasswordChangeRequest>,
) -> Result<Json<PasswordChangeResponse>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    let domain_ids: Vec<String> = store.domains.keys().cloned().collect();
    let created_tasks = domain_ids.len();

    for domain_id in domain_ids {
        let task_id = store.next_password_task_id;
        store.next_password_task_id += 1;
        store.password_tasks.push(PasswordTask {
            task_id,
            domain_id,
            employee_id: employee_id.clone(),
            encrypted_password: seal_password_for_storage(&request.password),
        });
    }
    record_audit(
        &mut store,
        "user:local",
        "password_tasks_created",
        format!("user:{employee_id}"),
        "accepted",
        BTreeMap::from([("created_tasks".to_string(), created_tasks.to_string())]),
    );

    Ok(Json(PasswordChangeResponse {
        employee_id,
        created_tasks,
    }))
}

async fn update_org_tree(
    State(state): State<AppState>,
    Json(mut desired_state): Json<DesiredState>,
) -> Result<Json<DesiredState>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    desired_state.version = store.desired_state.version + 1;
    store.desired_state = desired_state.clone();
    record_audit(
        &mut store,
        "user:local",
        "org_tree_updated",
        "org_tree",
        "accepted",
        BTreeMap::from([(
            "structure_version".to_string(),
            desired_state.version.to_string(),
        )]),
    );
    Ok(Json(desired_state))
}

async fn domain_status(
    State(state): State<AppState>,
    Path(domain_id): Path<String>,
) -> Result<Json<DomainSyncStatus>, ApiError> {
    let store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    if !store.domains.contains_key(&domain_id) {
        return Err(ApiError::NotFound);
    }

    let last_password_cursor = store
        .agent_cursors
        .iter()
        .filter(|(agent_id, _)| store.agents.get(*agent_id) == Some(&domain_id))
        .map(|(_, cursor)| *cursor)
        .max()
        .unwrap_or_default();
    let last_applied_structure_version = store
        .agent_structure_versions
        .iter()
        .filter(|(agent_id, _)| store.agents.get(*agent_id) == Some(&domain_id))
        .map(|(_, version)| *version)
        .max()
        .unwrap_or_default();
    let drift_count = store
        .drift_reports
        .iter()
        .filter(|report| report.domain_id == domain_id)
        .map(|report| report.drifted_objects.len())
        .sum();

    Ok(Json(DomainSyncStatus {
        domain_id,
        desired_structure_version: store.desired_state.version,
        last_applied_structure_version,
        last_password_cursor,
        drift_count,
    }))
}

async fn audit_events(State(state): State<AppState>) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    let store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    Ok(Json(store.audit_events.clone()))
}

async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> Result<Json<RegisterAgentResponse>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    let token_domain_id = store
        .registration_tokens
        .remove(&request.registration_token)
        .ok_or(ApiError::Unauthorized)?;

    if token_domain_id != request.domain_id || !store.domains.contains_key(&request.domain_id) {
        return Err(ApiError::Unauthorized);
    }

    store
        .agents
        .insert(request.agent_id.clone(), request.domain_id.clone());
    record_audit(
        &mut store,
        format!("agent:{}", request.agent_id),
        "agent_registered",
        format!("domain:{}", request.domain_id),
        "accepted",
        BTreeMap::from([(
            "certificate_subject".to_string(),
            request.certificate_subject,
        )]),
    );

    Ok(Json(RegisterAgentResponse {
        agent_id: request.agent_id,
        domain_id: request.domain_id,
    }))
}

async fn agent_poll(
    State(state): State<AppState>,
    Json(request): Json<AgentPollRequest>,
) -> Result<Json<AgentPollResponse>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    if let Err(error) = authorize_agent(&store, &request.agent_id, &request.domain_id) {
        record_audit(
            &mut store,
            format!("agent:{}", request.agent_id),
            "agent_poll_denied",
            format!("domain:{}", request.domain_id),
            "denied",
            BTreeMap::from([("reason".to_string(), error.audit_reason().to_string())]),
        );
        return Err(error);
    }

    let structure = if request.last_structure_version == store.desired_state.version {
        PollStructurePayload::NoChange
    } else if request.last_structure_version == 0
        || request.last_structure_version + 100 < store.desired_state.version
    {
        PollStructurePayload::Snapshot(store.desired_state.clone())
    } else {
        PollStructurePayload::Delta(store.desired_state.clone())
    };

    let password_tasks: Vec<PasswordTask> = store
        .password_tasks
        .iter()
        .filter(|task| {
            task.domain_id == request.domain_id && task.task_id > request.password_task_cursor
        })
        .cloned()
        .collect();

    let accepted_password_task_cursor = store
        .agent_cursors
        .get(&request.agent_id)
        .copied()
        .unwrap_or(request.password_task_cursor);
    let password_task_count = password_tasks.len();
    record_audit(
        &mut store,
        format!("agent:{}", request.agent_id),
        "agent_polled",
        format!("domain:{}", request.domain_id),
        "accepted",
        BTreeMap::from([
            (
                "last_structure_version".to_string(),
                request.last_structure_version.to_string(),
            ),
            (
                "password_task_count".to_string(),
                password_task_count.to_string(),
            ),
        ]),
    );

    Ok(Json(AgentPollResponse {
        structure,
        password_tasks,
        accepted_password_task_cursor,
    }))
}

async fn agent_report(
    State(state): State<AppState>,
    Json(request): Json<AgentReportRequest>,
) -> Result<Json<AgentReportResponse>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    authorize_agent(&store, &request.agent_id, &request.domain_id)?;
    store
        .agent_structure_versions
        .insert(request.agent_id.clone(), request.applied_structure_version);
    store
        .agent_cursors
        .insert(request.agent_id, request.applied_password_task_cursor);
    record_audit(
        &mut store,
        "agent:report",
        "agent_report_accepted",
        format!("domain:{}", request.domain_id),
        "accepted",
        BTreeMap::from([
            (
                "applied_structure_version".to_string(),
                request.applied_structure_version.to_string(),
            ),
            (
                "applied_password_task_cursor".to_string(),
                request.applied_password_task_cursor.to_string(),
            ),
        ]),
    );
    Ok(Json(AgentReportResponse { accepted: true }))
}

async fn drift_report(
    State(state): State<AppState>,
    Json(request): Json<DriftReportRequest>,
) -> Result<Json<StatusMessage>, ApiError> {
    let mut store = state.inner.lock().map_err(|_| ApiError::StatePoisoned)?;
    authorize_agent(&store, &request.agent_id, &request.domain_id)?;
    let drift_count = request.drifted_objects.len();
    let domain_id = request.domain_id.clone();
    let agent_id = request.agent_id.clone();
    store.drift_reports.push(request);
    record_audit(
        &mut store,
        format!("agent:{agent_id}"),
        "drift_report_recorded",
        format!("domain:{domain_id}"),
        "accepted",
        BTreeMap::from([("drift_count".to_string(), drift_count.to_string())]),
    );
    Ok(Json(StatusMessage::ok(
        "drift_recorded_without_reverse_write",
    )))
}

fn record_audit(
    store: &mut Store,
    actor: impl Into<String>,
    action: impl Into<String>,
    target: impl Into<String>,
    result: impl Into<String>,
    detail: BTreeMap<String, String>,
) {
    let sequence = store.next_audit_sequence;
    store.next_audit_sequence += 1;
    store.audit_events.push(AuditEvent {
        sequence,
        actor: actor.into(),
        action: action.into(),
        target: target.into(),
        result: result.into(),
        detail,
    });
}

fn authorize_agent(store: &Store, agent_id: &str, domain_id: &str) -> Result<(), ApiError> {
    match store.agents.get(agent_id) {
        Some(bound_domain_id) if bound_domain_id == domain_id => Ok(()),
        Some(_) => Err(ApiError::Forbidden),
        None => Err(ApiError::Unauthorized),
    }
}

fn seal_password_for_storage(password: &str) -> String {
    format!("kms:v1:len:{}", password.len())
}

#[derive(Debug, Serialize)]
struct StatusMessage {
    status: &'static str,
    message: String,
}

impl StatusMessage {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            status: "ok",
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DomainSyncStatus {
    domain_id: String,
    desired_structure_version: u64,
    last_applied_structure_version: u64,
    last_password_cursor: u64,
    drift_count: usize,
}

#[derive(Debug)]
enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    StatePoisoned,
}

impl ApiError {
    fn audit_reason(&self) -> &'static str {
        match self {
            ApiError::Unauthorized => "unauthorized_agent",
            ApiError::Forbidden => "domain_binding_mismatch",
            ApiError::NotFound => "not_found",
            ApiError::StatePoisoned => "state_poisoned",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::StatePoisoned => StatusCode::INTERNAL_SERVER_ERROR,
        };
        status.into_response()
    }
}
