use adss_contract::{AuditEvent, DesiredState, DomainConfig, OrgUnit};
use adss_persistence::{AgentCursorRecord, DriftReportRecord, OrmRepository, StoreSnapshot};
use std::collections::BTreeMap;

#[tokio::test]
async fn orm_repository_persists_and_loads_store_snapshot() {
    let repository = OrmRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();

    let snapshot = StoreSnapshot {
        desired_state: DesiredState {
            version: 42,
            ous: vec![OrgUnit {
                id: "engineering".to_string(),
                relative_dn: "OU=Engineering".to_string(),
            }],
            groups: Vec::new(),
            users: Vec::new(),
            memberships: Vec::new(),
        },
        domains: vec![DomainConfig::example()],
    };

    repository.save_snapshot(&snapshot).await.unwrap();
    let loaded = repository.load_snapshot().await.unwrap().unwrap();

    assert_eq!(loaded, snapshot);
}

#[tokio::test]
async fn orm_repository_appends_audit_events_in_sequence_order() {
    let repository = OrmRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();

    repository
        .append_audit_event(&AuditEvent {
            sequence: 1,
            actor: "agent:agent-a".to_string(),
            action: "agent_polled".to_string(),
            target: "domain:domain-a".to_string(),
            result: "accepted".to_string(),
            detail: BTreeMap::from([("password_task_count".to_string(), "0".to_string())]),
        })
        .await
        .unwrap();
    repository
        .append_audit_event(&AuditEvent {
            sequence: 2,
            actor: "user:local".to_string(),
            action: "password_tasks_created".to_string(),
            target: "user:E001".to_string(),
            result: "accepted".to_string(),
            detail: BTreeMap::from([("created_tasks".to_string(), "2".to_string())]),
        })
        .await
        .unwrap();

    let events = repository.list_audit_events().await.unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].action, "password_tasks_created");
    assert_eq!(events[1].detail["created_tasks"], "2");
}

#[tokio::test]
async fn orm_repository_persists_password_tasks_and_filters_by_domain_cursor() {
    let repository = OrmRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();

    repository
        .append_password_task(7, "domain-a", "E001", "kms:v1:ciphertext-a")
        .await
        .unwrap();
    repository
        .append_password_task(8, "domain-b", "E001", "kms:v1:ciphertext-b")
        .await
        .unwrap();
    repository
        .append_password_task(9, "domain-a", "E002", "kms:v1:ciphertext-c")
        .await
        .unwrap();

    let tasks = repository
        .list_password_tasks_after("domain-a", 7)
        .await
        .unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, 9);
    assert_eq!(tasks[0].domain_id, "domain-a");
    assert_eq!(tasks[0].employee_id, "E002");
}

#[tokio::test]
async fn orm_repository_upserts_agent_cursor_and_records_drift() {
    let repository = OrmRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();

    repository
        .upsert_agent_cursor(&AgentCursorRecord {
            agent_id: "agent-a".to_string(),
            domain_id: "domain-a".to_string(),
            structure_version: 11,
            password_task_cursor: 7,
        })
        .await
        .unwrap();
    repository
        .upsert_agent_cursor(&AgentCursorRecord {
            agent_id: "agent-a".to_string(),
            domain_id: "domain-a".to_string(),
            structure_version: 12,
            password_task_cursor: 9,
        })
        .await
        .unwrap();

    let cursor = repository
        .load_agent_cursor("agent-a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cursor.structure_version, 12);
    assert_eq!(cursor.password_task_cursor, 9);

    repository
        .append_drift_report(&DriftReportRecord {
            id: 1,
            domain_id: "domain-a".to_string(),
            agent_id: "agent-a".to_string(),
            observed_structure_version: 12,
            drifted_objects: vec!["employee:E001:mail".to_string()],
        })
        .await
        .unwrap();

    let drift = repository.list_drift_reports("domain-a").await.unwrap();
    assert_eq!(drift.len(), 1);
    assert_eq!(drift[0].drifted_objects, vec!["employee:E001:mail"]);
}

#[tokio::test]
async fn orm_repository_consumes_registration_token_once() {
    let repository = OrmRepository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();

    repository
        .insert_registration_token("register-domain-a", "domain-a")
        .await
        .unwrap();

    let domain_id = repository
        .consume_registration_token("register-domain-a")
        .await
        .unwrap();
    let second_attempt = repository
        .consume_registration_token("register-domain-a")
        .await
        .unwrap();

    assert_eq!(domain_id.as_deref(), Some("domain-a"));
    assert_eq!(second_attempt, None);
}
