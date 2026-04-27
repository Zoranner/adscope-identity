use adss_contract::{AuditEvent, DesiredState, DomainConfig, OrgUnit};
use adss_persistence::{OrmRepository, StoreSnapshot};
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
