use adss_protocol::{Group, OrganizationalUnit, UserStatus};
use adss_store::{
    DomainRecord, Repository, UserContactPatch, UserCredentialInput, UserDirectoryPatch,
    UserListFilter,
};
use sea_orm::{ConnectionTrait, Database};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn repository_updates_directory_objects_in_one_revision() {
    let repository = sqlite_repository().await;

    let revision = repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![UserDirectoryPatch {
                employee_id: "1001".to_string(),
                username: "zhangsan".to_string(),
                display_name: "张三".to_string(),
                email: Some("zhangsan@example.com".to_string()),
                mobile: None,
                telephone: None,
                organizational_unit_id: "ou-rd".to_string(),
                status: UserStatus::Active,
            }],
            vec![Group {
                id: "dev".to_string(),
                name: "Developers".to_string(),
                organizational_unit_id: "ou-rd".to_string(),
                member_employee_ids: vec!["1001".to_string()],
                changed_revision: 0,
            }],
        )
        .await
        .unwrap();

    let batch = repository
        .list_directory_changed_after("domain-a", 0, false, 100)
        .await
        .unwrap();

    assert_eq!(revision, 1);
    assert_eq!(batch.server_revision, 1);
    assert_eq!(batch.batch_revision, 1);
    assert_eq!(batch.organizational_units[0].changed_revision, 1);
    assert_eq!(batch.users[0].changed_revision, 1);
    assert_eq!(batch.groups[0].organizational_unit_id, "ou-rd");
    assert_eq!(batch.groups[0].changed_revision, 1);
    assert!(!batch.has_more);
}

#[tokio::test]
async fn repository_gets_user_by_username() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch_with_username(
                "1001",
                "zhangsan",
                "zhangsan@example.com",
            )],
            Vec::new(),
        )
        .await
        .unwrap();

    let user = repository
        .get_user_by_username("zhangsan")
        .await
        .unwrap()
        .unwrap();
    let missing = repository.get_user_by_username("lisi").await.unwrap();

    assert_eq!(user.employee_id, "1001");
    assert_eq!(user.username, "zhangsan");
    assert!(missing.is_none());
}

#[tokio::test]
async fn repository_rejects_duplicate_usernames() {
    let repository = sqlite_repository().await;

    let result = repository
        .upsert_directory(
            Vec::new(),
            vec![
                user_patch_with_username("1001", "zhangsan", "first@example.com"),
                user_patch_with_username("1002", "zhangsan", "second@example.com"),
            ],
            Vec::new(),
        )
        .await;
    let users = repository
        .list_users(UserListFilter::default())
        .await
        .unwrap();

    assert!(result.is_err());
    assert!(users.is_empty());
}

#[tokio::test]
async fn repository_returns_current_state_after_multiple_updates() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "first@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "latest@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();

    let batch = repository
        .list_directory_changed_after("domain-a", 0, false, 100)
        .await
        .unwrap();

    assert_eq!(batch.users.len(), 1);
    assert_eq!(batch.users[0].email.as_deref(), Some("latest@example.com"));
    assert_eq!(batch.users[0].changed_revision, 2);
    assert_eq!(batch.server_revision, 2);
    assert_eq!(batch.batch_revision, 2);
}

#[tokio::test]
async fn repository_updates_only_user_contact_fields() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            Vec::new(),
            vec![UserDirectoryPatch {
                mobile: Some("13800000000".to_string()),
                telephone: Some("021-10000000".to_string()),
                ..user_patch("1001", "old@example.com")
            }],
            Vec::new(),
        )
        .await
        .unwrap();
    let (user, revision) = repository
        .update_user_contact(
            "1001",
            UserContactPatch {
                email: Some("new@example.com".to_string()),
                mobile: Some("13900000000".to_string()),
                telephone: Some("021-20000000".to_string()),
            },
        )
        .await
        .unwrap();

    assert_eq!(revision, 2);
    assert_eq!(user.employee_id, "1001");
    assert_eq!(user.username, "1001");
    assert_eq!(user.display_name, "1001");
    assert_eq!(user.email.as_deref(), Some("new@example.com"));
    assert_eq!(user.mobile.as_deref(), Some("13900000000"));
    assert_eq!(user.telephone.as_deref(), Some("021-20000000"));
    assert_eq!(user.organizational_unit_id, "ou-rd");
    assert_eq!(user.status, UserStatus::Active);
    assert_eq!(user.changed_revision, 2);
}

#[tokio::test]
async fn repository_returns_all_ous_when_user_changes_after_confirmed_revision() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![user_patch("1001", "first@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .confirm_directory_revision("domain-a", 1)
        .await
        .unwrap();
    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "latest@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();

    let batch = repository
        .list_directory_changed_after("domain-a", 1, false, 100)
        .await
        .unwrap();

    assert_eq!(batch.organizational_units.len(), 1);
    assert_eq!(batch.organizational_units[0].id, "ou-rd");
    assert_eq!(batch.users.len(), 1);
    assert_eq!(batch.users[0].email.as_deref(), Some("latest@example.com"));
    assert_eq!(batch.batch_revision, 2);
}

#[tokio::test]
async fn repository_returns_all_ous_when_child_ou_changes_after_confirmed_revision() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            vec![
                OrganizationalUnit {
                    id: "ou-root".to_string(),
                    name: "Root".to_string(),
                    parent_id: None,
                    changed_revision: 0,
                },
                OrganizationalUnit {
                    id: "ou-child".to_string(),
                    name: "Child".to_string(),
                    parent_id: Some("ou-root".to_string()),
                    changed_revision: 0,
                },
            ],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .confirm_directory_revision("domain-a", 1)
        .await
        .unwrap();
    repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-child".to_string(),
                name: "Renamed Child".to_string(),
                parent_id: Some("ou-root".to_string()),
                changed_revision: 0,
            }],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

    let batch = repository
        .list_directory_changed_after("domain-a", 1, false, 100)
        .await
        .unwrap();

    assert_eq!(batch.organizational_units.len(), 2);
    assert_eq!(batch.organizational_units[0].id, "ou-root");
    assert_eq!(batch.organizational_units[1].id, "ou-child");
    assert_eq!(batch.organizational_units[1].name, "Renamed Child");
}

#[tokio::test]
async fn repository_returns_all_pending_directory_objects_even_when_limit_is_one() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            vec![
                OrganizationalUnit {
                    id: "ou-root".to_string(),
                    name: "Root".to_string(),
                    parent_id: None,
                    changed_revision: 0,
                },
                OrganizationalUnit {
                    id: "ou-child".to_string(),
                    name: "Child".to_string(),
                    parent_id: Some("ou-root".to_string()),
                    changed_revision: 0,
                },
            ],
            vec![user_patch("1001", "zhangsan@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1002", "lisi@example.com")],
            vec![Group {
                id: "ops".to_string(),
                name: "Operators".to_string(),
                organizational_unit_id: "ou-root".to_string(),
                member_employee_ids: vec!["1002".to_string()],
                changed_revision: 0,
            }],
        )
        .await
        .unwrap();

    let batch = repository
        .list_directory_changed_after("domain-a", 0, false, 1)
        .await
        .unwrap();

    assert_eq!(batch.server_revision, 2);
    assert_eq!(batch.batch_revision, 2);
    assert_eq!(batch.organizational_units.len(), 2);
    assert_eq!(batch.users.len(), 2);
    assert_eq!(batch.groups.len(), 1);
    assert!(!batch.has_more);
}

#[tokio::test]
async fn repository_allocates_continuous_revisions_for_repeated_writes() {
    let repository = sqlite_repository().await;

    let first_directory = repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "first@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    let second_directory = repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1002", "second@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    let first_credential = repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:first".to_string(),
            password_verifier: "verify:first".to_string(),
        })
        .await
        .unwrap();
    let second_credential = repository
        .change_user_password(UserCredentialInput {
            employee_id: "1002".to_string(),
            password_ciphertext: "cipher:second".to_string(),
            password_verifier: "verify:second".to_string(),
        })
        .await
        .unwrap();

    assert_eq!((first_directory, second_directory), (1, 2));
    assert_eq!((first_credential, second_credential), (1, 2));
}

#[tokio::test]
async fn repository_updates_domain_revision_only_on_confirmed_channel() {
    let repository = sqlite_repository().await;

    for index in 1..=7 {
        repository
            .upsert_directory(
                Vec::new(),
                vec![user_patch(
                    &format!("directory-{index}"),
                    &format!("directory-{index}@example.com"),
                )],
                Vec::new(),
            )
            .await
            .unwrap();
    }
    for index in 1..=3 {
        repository
            .change_user_password(UserCredentialInput {
                employee_id: format!("credential-{index}"),
                password_ciphertext: format!("cipher:{index}"),
                password_verifier: format!("verify:{index}"),
            })
            .await
            .unwrap();
    }

    repository
        .confirm_directory_revision("domain-a", 7)
        .await
        .unwrap();
    repository
        .confirm_credential_revision("domain-a", 3)
        .await
        .unwrap();
    assert!(
        repository
            .confirm_directory_revision("domain-a", 6)
            .await
            .is_err()
    );
    let domain = repository.get_domain("domain-a").await.unwrap().unwrap();

    assert_eq!(domain.applied_directory_revision, 7);
    assert_eq!(domain.applied_credential_revision, 3);
}

#[tokio::test]
async fn repository_preserves_both_channels_after_concurrent_confirmations() {
    let (database_url, database_path) =
        sqlite_file_database_url("concurrent_channel_confirmations");
    let repository = sqlite_file_repository(&database_url).await;

    seed_directory_revisions(&repository, 7).await;
    seed_credential_revisions(&repository, 3).await;

    let directory_repository = Repository::connect(&database_url).await.unwrap();
    let credential_repository = Repository::connect(&database_url).await.unwrap();
    let (directory_result, credential_result) = tokio::join!(
        directory_repository.confirm_directory_revision("domain-a", 7),
        credential_repository.confirm_credential_revision("domain-a", 3)
    );

    directory_result.unwrap();
    credential_result.unwrap();
    let domain = repository.get_domain("domain-a").await.unwrap().unwrap();

    assert_eq!(domain.applied_directory_revision, 7);
    assert_eq!(domain.applied_credential_revision, 3);

    drop(repository);
    drop(directory_repository);
    drop(credential_repository);
    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
async fn repository_same_channel_concurrent_confirmations_do_not_move_backward() {
    let (database_url, database_path) =
        sqlite_file_database_url("same_channel_concurrent_confirmations");
    let repository = sqlite_file_repository(&database_url).await;

    seed_directory_revisions(&repository, 7).await;

    let newer_repository = Repository::connect(&database_url).await.unwrap();
    let older_repository = Repository::connect(&database_url).await.unwrap();
    let (newer_result, older_result) = tokio::join!(
        newer_repository.confirm_directory_revision("domain-a", 7),
        older_repository.confirm_directory_revision("domain-a", 6)
    );

    newer_result.unwrap();
    let _ = older_result;
    assert!(
        repository
            .confirm_directory_revision("domain-a", 6)
            .await
            .is_err()
    );
    let domain = repository.get_domain("domain-a").await.unwrap().unwrap();

    assert_eq!(domain.applied_directory_revision, 7);
    assert_eq!(domain.applied_credential_revision, 0);

    drop(repository);
    drop(newer_repository);
    drop(older_repository);
    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
async fn repository_rejects_confirm_revision_outside_channel_bounds() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "zhangsan@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .confirm_directory_revision("domain-a", 1)
        .await
        .unwrap();

    assert!(
        repository
            .confirm_directory_revision("domain-a", 0)
            .await
            .is_err()
    );
    assert!(
        repository
            .confirm_credential_revision("domain-a", 1)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn repository_rejects_pull_revision_ahead_of_domain_progress() {
    let repository = sqlite_repository().await;

    repository
        .upsert_directory(
            Vec::new(),
            vec![user_patch("1001", "zhangsan@example.com")],
            Vec::new(),
        )
        .await
        .unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:first".to_string(),
            password_verifier: "verify:first".to_string(),
        })
        .await
        .unwrap();

    assert!(
        repository
            .list_directory_changed_after("domain-a", 1, false, 100)
            .await
            .is_err()
    );
    assert!(
        repository
            .list_credentials_changed_after("domain-a", 1, false, 100)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn repository_schema_rejects_negative_revisions() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let repository = Repository::from_connection(db.clone());
    repository.initialize_schema().await.unwrap();

    assert!(
        db.execute_unprepared(
            "UPDATE sync_metadata SET directory_revision = -1 WHERE key = 'current'"
        )
        .await
        .is_err()
    );
    assert!(
        db.execute_unprepared(
            "INSERT INTO domains (
                id,
                name,
                enabled,
                mirror_root_dn,
                quarantine_ou_dn,
                upn_suffix,
                employee_id_attribute,
                managed_group_id_attribute,
                connector_key_hash,
                applied_directory_revision,
                applied_credential_revision
            ) VALUES (
                'bad-domain',
                'Bad Domain',
                1,
                'OU=Mirror,DC=bad,DC=example,DC=com',
                'OU=Quarantine,DC=bad,DC=example,DC=com',
                'bad.example.com',
                'employeeID',
                'adminDescription',
                'hash:connector-key',
                -1,
                0
            )"
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn repository_stores_only_current_credential() {
    let repository = sqlite_repository().await;

    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:first".to_string(),
            password_verifier: "verify:first".to_string(),
        })
        .await
        .unwrap();
    repository
        .change_user_password(UserCredentialInput {
            employee_id: "1001".to_string(),
            password_ciphertext: "cipher:latest".to_string(),
            password_verifier: "verify:latest".to_string(),
        })
        .await
        .unwrap();

    let credentials = repository
        .list_credentials_changed_after("domain-a", 0, false, 100)
        .await
        .unwrap();

    assert_eq!(credentials.credentials.len(), 1);
    assert_eq!(credentials.credentials[0].employee_id, "1001");
    assert_eq!(
        credentials.credentials[0].password_ciphertext,
        "cipher:latest"
    );
    assert_eq!(credentials.credentials[0].changed_revision, 2);
    assert_eq!(
        repository
            .get_credential_record("1001")
            .await
            .unwrap()
            .unwrap()
            .password_ciphertext,
        "cipher:latest"
    );
}

#[tokio::test]
async fn repository_schema_uses_group_ou_and_member_employee_ids_column_names() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let repository = Repository::from_connection(db.clone());
    repository.initialize_schema().await.unwrap();

    db.execute_unprepared(
        "INSERT INTO groups (
            id,
            name,
            organizational_unit_id,
            member_employee_ids,
            changed_revision
        ) VALUES (
            'schema-check',
            'Schema Check',
            'ou-root',
            '[\"1001\"]',
            0
        )",
    )
    .await
    .unwrap();

    assert!(
        db.execute_unprepared(
            "INSERT INTO groups (
                id,
                name,
                organizational_unit_id,
                member_employee_ids_json,
                changed_revision
            ) VALUES (
                'old-schema-check',
                'Old Schema Check',
                'ou-root',
                '[\"1001\"]',
                0
            )"
        )
        .await
        .is_err()
    );
}

async fn sqlite_repository() -> Repository {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain()).await.unwrap();
    repository
}

async fn sqlite_file_repository(database_url: &str) -> Repository {
    let repository = Repository::connect(database_url).await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository.seed_domain(domain()).await.unwrap();
    repository
}

fn sqlite_file_database_url(name: &str) -> (String, PathBuf) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let database_path = std::env::temp_dir().join(format!(
        "adss-store-{name}-{timestamp}-{}.db",
        std::process::id()
    ));
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        database_path.to_string_lossy().replace('\\', "/")
    );
    (database_url, database_path)
}

async fn seed_directory_revisions(repository: &Repository, count: u64) {
    for index in 1..=count {
        repository
            .upsert_directory(
                Vec::new(),
                vec![user_patch(
                    &format!("directory-{index}"),
                    &format!("directory-{index}@example.com"),
                )],
                Vec::new(),
            )
            .await
            .unwrap();
    }
}

async fn seed_credential_revisions(repository: &Repository, count: u64) {
    for index in 1..=count {
        repository
            .change_user_password(UserCredentialInput {
                employee_id: format!("credential-{index}"),
                password_ciphertext: format!("cipher:{index}"),
                password_verifier: format!("verify:{index}"),
            })
            .await
            .unwrap();
    }
}

fn domain() -> DomainRecord {
    DomainRecord {
        id: "domain-a".to_string(),
        name: "Domain A".to_string(),
        enabled: true,
        mirror_root_dn: "OU=Mirror,DC=a,DC=example,DC=com".to_string(),
        quarantine_ou_dn: "OU=Quarantine,DC=a,DC=example,DC=com".to_string(),
        upn_suffix: "a.example.com".to_string(),
        employee_id_attribute: "employeeID".to_string(),
        managed_group_id_attribute: "adminDescription".to_string(),
        connector_key_hash: "hash:connector-key".to_string(),
        applied_directory_revision: 0,
        applied_credential_revision: 0,
    }
}

fn user_patch(employee_id: &str, email: &str) -> UserDirectoryPatch {
    user_patch_with_username(employee_id, employee_id, email)
}

fn user_patch_with_username(employee_id: &str, username: &str, email: &str) -> UserDirectoryPatch {
    UserDirectoryPatch {
        employee_id: employee_id.to_string(),
        username: username.to_string(),
        display_name: employee_id.to_string(),
        email: Some(email.to_string()),
        mobile: None,
        telephone: None,
        organizational_unit_id: "ou-rd".to_string(),
        status: UserStatus::Active,
    }
}
