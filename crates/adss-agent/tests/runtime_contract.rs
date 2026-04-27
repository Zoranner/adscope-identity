use adss_agent::{AgentCursor, AgentRuntime, ControlPlaneClient, DirectoryClient};
use adss_contract::{
    AdOperation, AdOperationKind, AgentPollRequest, AgentPollResponse, AgentReportRequest,
    AgentReportResponse, DesiredState, DomainConfig, OrgUnit, PasswordTask, PollStructurePayload,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn runtime_polls_executes_structure_then_password_tasks_and_reports_cursors() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let directory = RecordingDirectory {
        calls: Arc::clone(&calls),
    };
    let control_plane = RecordingControlPlane {
        reports: Arc::clone(&reports),
    };
    let mut runtime = AgentRuntime::new(
        "domain-a",
        "agent-a",
        DomainConfig::example(),
        AgentCursor {
            structure_version: 0,
            password_task_cursor: 0,
        },
        control_plane,
        directory,
    );

    let summary = runtime.run_once().await.unwrap();

    assert_eq!(summary.succeeded, 2);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            "operation:ensure_ou:OU=Engineering".to_string(),
            "password:7:E001".to_string(),
        ]
    );

    let reports = reports.lock().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].applied_structure_version, 9);
    assert_eq!(reports[0].applied_password_task_cursor, 7);
    assert_eq!(runtime.cursor().structure_version, 9);
    assert_eq!(runtime.cursor().password_task_cursor, 7);
}

#[derive(Clone)]
struct RecordingControlPlane {
    reports: Arc<Mutex<Vec<AgentReportRequest>>>,
}

#[async_trait]
impl ControlPlaneClient for RecordingControlPlane {
    async fn poll(&self, request: AgentPollRequest) -> anyhow::Result<AgentPollResponse> {
        assert_eq!(request.domain_id, "domain-a");
        assert_eq!(request.agent_id, "agent-a");
        assert_eq!(request.last_structure_version, 0);
        assert_eq!(request.password_task_cursor, 0);

        Ok(AgentPollResponse {
            structure: PollStructurePayload::Snapshot(DesiredState {
                version: 9,
                ous: vec![OrgUnit {
                    id: "engineering".to_string(),
                    relative_dn: "OU=Engineering".to_string(),
                }],
                groups: Vec::new(),
                users: Vec::new(),
                memberships: Vec::new(),
            }),
            password_tasks: vec![PasswordTask {
                task_id: 7,
                domain_id: "domain-a".to_string(),
                employee_id: "E001".to_string(),
                encrypted_password: "kms:v1:ciphertext".to_string(),
            }],
            accepted_password_task_cursor: 0,
        })
    }

    async fn report(&self, request: AgentReportRequest) -> anyhow::Result<AgentReportResponse> {
        self.reports.lock().unwrap().push(request);
        Ok(AgentReportResponse { accepted: true })
    }
}

struct RecordingDirectory {
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl DirectoryClient for RecordingDirectory {
    async fn apply(&self, operation: &AdOperation) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!(
            "operation:{}:{}",
            operation_kind_name(operation.kind),
            operation.subject
        ));
        Ok(())
    }

    async fn set_password(&self, task: &PasswordTask) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("password:{}:{}", task.task_id, task.employee_id));
        Ok(())
    }
}

fn operation_kind_name(kind: AdOperationKind) -> &'static str {
    match kind {
        AdOperationKind::EnsureOu => "ensure_ou",
        AdOperationKind::EnsureGroup => "ensure_group",
        AdOperationKind::EnsureUser => "ensure_user",
        AdOperationKind::EnsureUserPlacement => "ensure_user_placement",
        AdOperationKind::EnsureGroupMembership => "ensure_group_membership",
        AdOperationKind::DisableUser => "disable_user",
        AdOperationKind::MoveUserToQuarantine => "move_user_to_quarantine",
        AdOperationKind::DeleteUser => "delete_user",
    }
}
