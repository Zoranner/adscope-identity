use adss_contract::{
    AdOperationKind, DesiredState, DomainConfig, Group, GroupMembership, OrgUnit, ReconcilePlan,
    User, UserStatus, sanitize_user_attributes,
};
use std::collections::BTreeMap;

#[test]
fn sanitize_user_attributes_keeps_only_whitelisted_non_system_fields() {
    let mut input = BTreeMap::new();
    input.insert("displayName".to_string(), "张三".to_string());
    input.insert("mail".to_string(), "zhangsan@example.com".to_string());
    input.insert("objectGUID".to_string(), "must-not-copy".to_string());
    input.insert("objectSid".to_string(), "must-not-copy".to_string());
    input.insert("telephoneNumber".to_string(), "1234".to_string());

    let sanitized = sanitize_user_attributes(input, &["displayName", "mail"]);

    assert_eq!(sanitized.len(), 2);
    assert_eq!(sanitized["displayName"], "张三");
    assert_eq!(sanitized["mail"], "zhangsan@example.com");
    assert!(!sanitized.contains_key("objectGUID"));
    assert!(!sanitized.contains_key("objectSid"));
    assert!(!sanitized.contains_key("telephoneNumber"));
}

#[test]
fn reconcile_plan_uses_required_execution_order() {
    let state = DesiredState {
        version: 7,
        ous: vec![OrgUnit {
            id: "ou-engineering".to_string(),
            relative_dn: "OU=研发中心".to_string(),
        }],
        groups: vec![Group {
            id: "group-rust".to_string(),
            sam_account_name: "rust-dev".to_string(),
            relative_dn: "CN=rust-dev,OU=研发中心".to_string(),
        }],
        users: vec![User {
            employee_id: "E001".to_string(),
            sam_account_name: "zhangsan".to_string(),
            upn: "zhangsan@example.com".to_string(),
            display_name: "张三".to_string(),
            relative_dn: "CN=张三,OU=研发中心".to_string(),
            status: UserStatus::Active,
            attributes: BTreeMap::new(),
        }],
        memberships: vec![GroupMembership {
            group_id: "group-rust".to_string(),
            member_employee_id: "E001".to_string(),
        }],
    };

    let plan = ReconcilePlan::from_desired_state(&state, &DomainConfig::example());
    let kinds: Vec<_> = plan.operations.iter().map(|op| op.kind).collect();

    assert_eq!(
        kinds,
        vec![
            AdOperationKind::EnsureOu,
            AdOperationKind::EnsureGroup,
            AdOperationKind::EnsureUser,
            AdOperationKind::EnsureUserPlacement,
            AdOperationKind::EnsureGroupMembership,
        ]
    );
}

#[test]
fn deleted_users_are_disabled_and_moved_to_quarantine_instead_of_deleted() {
    let state = DesiredState {
        version: 8,
        ous: Vec::new(),
        groups: Vec::new(),
        users: vec![User {
            employee_id: "E002".to_string(),
            sam_account_name: "lisi".to_string(),
            upn: "lisi@example.com".to_string(),
            display_name: "李四".to_string(),
            relative_dn: "CN=李四,OU=研发中心".to_string(),
            status: UserStatus::DeletedPendingIsolation,
            attributes: BTreeMap::new(),
        }],
        memberships: Vec::new(),
    };

    let config = DomainConfig::example();
    let plan = ReconcilePlan::from_desired_state(&state, &config);
    let kinds: Vec<_> = plan.operations.iter().map(|op| op.kind).collect();

    assert_eq!(
        kinds,
        vec![
            AdOperationKind::DisableUser,
            AdOperationKind::MoveUserToQuarantine,
        ]
    );
    assert_eq!(
        plan.operations[1].target_dn.as_deref(),
        Some(config.quarantine_ou_dn.as_str())
    );
    assert!(!kinds.contains(&AdOperationKind::DeleteUser));
}
