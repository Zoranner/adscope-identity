use adss_contract::{
    AgentPollRequest, AgentReportRequest, DriftReportRequest, PasswordChangeRequest,
    PollStructurePayload, RegisterAgentRequest, UpdateUserRequest,
};
use adss_server::{AppState, build_router};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use std::collections::BTreeMap;
use tower::ServiceExt;

#[tokio::test]
async fn agent_cannot_poll_another_domain_tasks() {
    let state = AppState::seeded();
    let app = build_router(state);

    let request = AgentPollRequest {
        domain_id: "domain-b".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 0,
        password_task_cursor: 0,
    };

    let response = app
        .oneshot(json_request("/api/agent/poll", &request))
        .await
        .expect("poll response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn user_update_changes_desired_state_and_filters_non_whitelisted_attributes() {
    let state = AppState::seeded();
    let app = build_router(state);

    let request = UpdateUserRequest {
        display_name: Some("张三丰".to_string()),
        relative_dn: Some("CN=张三丰,OU=Engineering".to_string()),
        attributes: BTreeMap::from([
            ("mail".to_string(), "zhangsanfeng@example.com".to_string()),
            ("telephoneNumber".to_string(), "1001".to_string()),
            ("objectGUID".to_string(), "must-not-copy".to_string()),
        ]),
    };

    let response = app
        .clone()
        .oneshot(method_json_request(
            Method::PATCH,
            "/api/users/E001",
            &request,
        ))
        .await
        .expect("update response");

    assert_eq!(response.status(), StatusCode::OK);

    let poll = AgentPollRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 3,
        password_task_cursor: 0,
    };
    let response = app
        .oneshot(json_request("/api/agent/poll", &poll))
        .await
        .expect("poll response");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let poll: adss_contract::AgentPollResponse = serde_json::from_slice(&body).unwrap();
    let PollStructurePayload::Delta(state) = poll.structure else {
        panic!("expected structure delta after user update");
    };
    let user = state
        .users
        .iter()
        .find(|user| user.employee_id == "E001")
        .expect("updated user");

    assert_eq!(state.version, 4);
    assert_eq!(user.display_name, "张三丰");
    assert_eq!(user.attributes["mail"], "zhangsanfeng@example.com");
    assert_eq!(user.attributes["telephoneNumber"], "1001");
    assert!(!user.attributes.contains_key("objectGUID"));
}

#[tokio::test]
async fn agent_registration_binds_domain_and_consumes_one_time_token() {
    let state = AppState::seeded();
    let app = build_router(state);

    let request = RegisterAgentRequest {
        registration_token: "register-domain-a".to_string(),
        agent_id: "agent-new".to_string(),
        domain_id: "domain-a".to_string(),
        certificate_subject: "CN=agent-new".to_string(),
    };

    let response = app
        .clone()
        .oneshot(json_request("/api/agent/register", &request))
        .await
        .expect("register response");
    assert_eq!(response.status(), StatusCode::OK);

    let poll = AgentPollRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-new".to_string(),
        last_structure_version: 0,
        password_task_cursor: 0,
    };
    let response = app
        .clone()
        .oneshot(json_request("/api/agent/poll", &poll))
        .await
        .expect("poll response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(json_request("/api/agent/register", &request))
        .await
        .expect("register response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn drift_report_is_visible_in_domain_status_without_changing_desired_state() {
    let state = AppState::seeded();
    let app = build_router(state);

    let drift = DriftReportRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        observed_structure_version: 3,
        drifted_objects: vec!["employee:E001:mail".to_string()],
    };

    let response = app
        .clone()
        .oneshot(json_request("/api/agent/drift-report", &drift))
        .await
        .expect("drift response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(empty_request(
            Method::GET,
            "/api/sync/domains/domain-a/status",
        ))
        .await
        .expect("status response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(text.contains("\"drift_count\":1"));

    let poll = AgentPollRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 3,
        password_task_cursor: 0,
    };
    let response = app
        .oneshot(json_request("/api/agent/poll", &poll))
        .await
        .expect("poll response");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let poll: adss_contract::AgentPollResponse = serde_json::from_slice(&body).unwrap();
    assert!(matches!(poll.structure, PollStructurePayload::NoChange));
}

#[tokio::test]
async fn stale_agent_cursor_receives_full_snapshot() {
    let state = AppState::seeded();
    let app = build_router(state);

    let request = AgentPollRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 0,
        password_task_cursor: 0,
    };

    let response = app
        .oneshot(json_request("/api/agent/poll", &request))
        .await
        .expect("poll response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let poll: adss_contract::AgentPollResponse = serde_json::from_slice(&body).unwrap();

    assert!(matches!(poll.structure, PollStructurePayload::Snapshot(_)));
}

#[tokio::test]
async fn password_change_creates_domain_tasks_without_returning_plaintext() {
    let state = AppState::seeded();
    let app = build_router(state);

    let request = PasswordChangeRequest {
        password: "P@ssw0rd-never-leak".to_string(),
    };

    let response = app
        .oneshot(json_request("/api/users/E001/password", &request))
        .await
        .expect("password response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = std::str::from_utf8(&body).unwrap();

    assert!(!text.contains("P@ssw0rd-never-leak"));
    assert!(text.contains("\"created_tasks\":2"));
}

#[tokio::test]
async fn agent_report_advances_cursor_after_successful_password_task() {
    let state = AppState::seeded();
    let app = build_router(state);

    let report = AgentReportRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        applied_structure_version: 3,
        applied_password_task_cursor: 5,
        summary: adss_contract::SyncSummary {
            succeeded: 1,
            failed: 0,
            skipped: 0,
            pending_manual: 0,
        },
        object_results: Vec::new(),
    };

    let response = app
        .clone()
        .oneshot(json_request("/api/agent/report", &report))
        .await
        .expect("report response");

    assert_eq!(response.status(), StatusCode::OK);

    let poll = AgentPollRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 3,
        password_task_cursor: 5,
    };
    let response = app
        .oneshot(json_request("/api/agent/poll", &poll))
        .await
        .expect("poll response");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let poll: adss_contract::AgentPollResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(poll.accepted_password_task_cursor, 5);
}

#[tokio::test]
async fn audit_events_record_security_relevant_actions_without_plaintext_passwords() {
    let state = AppState::seeded();
    let app = build_router(state);

    let password = PasswordChangeRequest {
        password: "P@ssw0rd-never-leak".to_string(),
    };
    let response = app
        .clone()
        .oneshot(json_request("/api/users/E001/password", &password))
        .await
        .expect("password response");
    assert_eq!(response.status(), StatusCode::OK);

    let forbidden_poll = AgentPollRequest {
        domain_id: "domain-b".to_string(),
        agent_id: "agent-a".to_string(),
        last_structure_version: 0,
        password_task_cursor: 0,
    };
    let response = app
        .clone()
        .oneshot(json_request("/api/agent/poll", &forbidden_poll))
        .await
        .expect("poll response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let report = AgentReportRequest {
        domain_id: "domain-a".to_string(),
        agent_id: "agent-a".to_string(),
        applied_structure_version: 3,
        applied_password_task_cursor: 1,
        summary: adss_contract::SyncSummary {
            succeeded: 1,
            failed: 0,
            skipped: 0,
            pending_manual: 0,
        },
        object_results: Vec::new(),
    };
    let response = app
        .clone()
        .oneshot(json_request("/api/agent/report", &report))
        .await
        .expect("report response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(empty_request(Method::GET, "/api/audit/events"))
        .await
        .expect("audit response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = std::str::from_utf8(&body).unwrap();

    assert!(text.contains("\"action\":\"password_tasks_created\""));
    assert!(text.contains("\"action\":\"agent_poll_denied\""));
    assert!(text.contains("\"action\":\"agent_report_accepted\""));
    assert!(!text.contains("P@ssw0rd-never-leak"));
}

fn json_request<T: serde::Serialize>(uri: &str, value: &T) -> Request<Body> {
    method_json_request(Method::POST, uri, value)
}

fn method_json_request<T: serde::Serialize>(method: Method, uri: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

fn empty_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}
