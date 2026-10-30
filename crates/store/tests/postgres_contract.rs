use adscope_protocol::{OrganizationalUnit, UserStatus};
use adscope_store::{
    DomainRecord, Repository, UserCreateInput, UserCredentialInput, UserDirectoryPatch,
};

#[tokio::test]
#[ignore = "requires ADSCOPE_TEST_POSTGRES_URL pointing at a disposable PostgreSQL database"]
async fn postgres_runs_schema_and_write_paths() {
    let database_url = std::env::var("ADSCOPE_TEST_POSTGRES_URL")
        .expect("ADSCOPE_TEST_POSTGRES_URL must point at a disposable PostgreSQL database");
    let repository = Repository::connect(&database_url).await.unwrap();

    repository.initialize_schema().await.unwrap();
    // 建表逻辑必须可以重复执行。
    repository.initialize_schema().await.unwrap();

    repository.create_domain(domain()).await.unwrap();

    let revision = repository
        .upsert_directory(
            vec![OrganizationalUnit {
                id: "ou-rd".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 0,
            }],
            vec![directory_patch("1001", "zhangsan")],
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(revision, 1);

    assert_eq!(
        repository.preview_next_employee_id().await.unwrap(),
        "100001"
    );

    let (created, directory_revision, credential_revision) = repository
        .create_user_with_initial_credential(UserCreateInput {
            directory: directory_patch("", "lisi"),
            credential: UserCredentialInput {
                employee_id: String::new(),
                password_ciphertext: "ciphertext".to_string(),
                password_verifier: "verifier".to_string(),
            },
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.employee_id, "100001");
    assert_eq!(directory_revision, 2);
    assert_eq!(credential_revision, 1);

    assert!(repository.delete_user("100001").await.unwrap().is_some());

    let batch = repository
        .list_directory_changed_after("domain-pg", 0, true, 100)
        .await
        .unwrap();
    assert_eq!(batch.deleted_users.len(), 1);
    assert_eq!(batch.deleted_users[0].employee_id, "100001");
    assert!(batch.users.iter().any(|user| user.employee_id == "1001"));

    // 确认 revision 的 UPDATE 使用各自后端的占位符写法。
    repository
        .confirm_directory_revision("domain-pg", batch.batch_revision)
        .await
        .unwrap();
    repository
        .confirm_credential_revision("domain-pg", credential_revision)
        .await
        .unwrap();
    let domain = repository.get_domain("domain-pg").await.unwrap().unwrap();
    assert_eq!(domain.applied_directory_revision, batch.batch_revision);
    assert_eq!(domain.applied_credential_revision, credential_revision);
}

fn domain() -> DomainRecord {
    DomainRecord {
        id: "domain-pg".to_string(),
        name: "PostgreSQL".to_string(),
        enabled: true,
        mirror_root_dn: "OU=Mirror,DC=pg,DC=example,DC=com".to_string(),
        quarantine_ou_dn: "OU=Quarantine,DC=pg,DC=example,DC=com".to_string(),
        upn_suffix: "pg.example.com".to_string(),
        employee_id_attribute: "employeeID".to_string(),
        managed_group_id_attribute: "adminDescription".to_string(),
        syncer_key_hash: "hash:syncer-key".to_string(),
        applied_directory_revision: 0,
        applied_credential_revision: 0,
    }
}

fn directory_patch(employee_id: &str, username: &str) -> UserDirectoryPatch {
    UserDirectoryPatch {
        employee_id: employee_id.to_string(),
        username: username.to_string(),
        display_name: username.to_string(),
        surname: username.to_string(),
        given_name: String::new(),
        email: Some(format!("{username}@example.com")),
        mobile: None,
        telephone: None,
        organizational_unit_id: "ou-rd".to_string(),
        status: UserStatus::Active,
    }
}
