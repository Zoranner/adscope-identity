use adss_connector::{
    DirectoryBatchSession, DirectoryClient, DirectoryExecutionContext, DirectoryExecutor,
    encode_ad_unicode_password, escape_ldap_dn_value, escape_ldap_filter_value,
    execute_credential_batch, execute_directory_plan,
};
use adss_protocol::{
    CredentialBatch, CredentialEntry, DirectoryBatch, DirectoryOperation, DirectoryOperationKind,
    DirectoryPlan, DomainDirectoryConfig, Group, OrganizationalUnit, User, UserStatus,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn executor_runs_directory_plan_in_contract_order() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let opened_batches = Arc::new(Mutex::new(0));
    let client = RecordingDirectoryClient {
        calls: Arc::clone(&calls),
        opened_batches: Arc::clone(&opened_batches),
        fail_kind: None,
        fail_password_employee_id: None,
    };
    let executor = DirectoryExecutor::new(client);
    let plan =
        DirectoryPlan::try_from_batch(&directory_batch(11), &DomainDirectoryConfig::example())
            .unwrap();
    let context = DirectoryExecutionContext::try_from_batch(
        &directory_batch(11),
        &DomainDirectoryConfig::example(),
    )
    .unwrap();

    let result = executor.execute(&plan, &context).await.unwrap();

    assert_eq!(result.summary.succeeded, 5);
    assert!(result.failure.is_none());
    assert_eq!(*opened_batches.lock().unwrap(), 1);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            "EnsureOu".to_string(),
            "EnsureUser".to_string(),
            "EnsureUserPlacement".to_string(),
            "EnsureGroup".to_string(),
            "EnsureGroupMembers".to_string(),
        ]
    );
}

#[tokio::test]
async fn failed_directory_operation_is_reported_without_running_later_operations() {
    let client = RecordingDirectoryClient {
        calls: Arc::new(Mutex::new(Vec::new())),
        opened_batches: Arc::new(Mutex::new(0)),
        fail_kind: Some(DirectoryOperationKind::EnsureUser),
        fail_password_employee_id: None,
    };
    let plan =
        DirectoryPlan::try_from_batch(&directory_batch(12), &DomainDirectoryConfig::example())
            .unwrap();
    let context = DirectoryExecutionContext::try_from_batch(
        &directory_batch(12),
        &DomainDirectoryConfig::example(),
    )
    .unwrap();

    let result = execute_directory_plan(&client, &plan, &context).await;

    assert_eq!(result.summary.succeeded, 1);
    assert_eq!(result.summary.failed, 1);
    assert_eq!(result.summary.skipped, 3);
    let failure = result.failure.unwrap();
    assert_eq!(failure.operation, "ensure_user");
    assert_eq!(failure.subject, "1001");
    assert!(failure.detail.contains("LDAP permission denied"));
}

#[tokio::test]
async fn credential_batch_stops_at_first_password_failure() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let opened_batches = Arc::new(Mutex::new(0));
    let client = RecordingDirectoryClient {
        calls: Arc::clone(&calls),
        opened_batches: Arc::clone(&opened_batches),
        fail_kind: None,
        fail_password_employee_id: Some("1002"),
    };
    let batch = CredentialBatch {
        server_revision: 3,
        batch_revision: 3,
        credentials: vec![
            CredentialEntry {
                employee_id: "1001".to_string(),
                plaintext_password: "FirstPass123!".to_string(),
                status: UserStatus::Active,
                changed_revision: 1,
            },
            CredentialEntry {
                employee_id: "1002".to_string(),
                plaintext_password: "SecondPass123!".to_string(),
                status: UserStatus::Active,
                changed_revision: 2,
            },
            CredentialEntry {
                employee_id: "1003".to_string(),
                plaintext_password: "ThirdPass123!".to_string(),
                status: UserStatus::Active,
                changed_revision: 3,
            },
        ],
        has_more: false,
    };

    let context = DirectoryExecutionContext::from_domain(&DomainDirectoryConfig::example());

    let result = execute_credential_batch(&client, &batch, &context).await;

    assert_eq!(result.summary.succeeded, 1);
    assert_eq!(result.summary.failed, 1);
    assert_eq!(result.summary.skipped, 1);
    let failure = result.failure.unwrap();
    assert_eq!(failure.operation, "set_password");
    assert_eq!(failure.subject, "1002");
    assert!(failure.detail.contains("password denied"));
    assert!(!failure.detail.contains("SecondPass123!"));
    assert_eq!(*opened_batches.lock().unwrap(), 1);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &["password:1001".to_string(), "password:1002".to_string()]
    );
}

#[tokio::test]
async fn directory_executor_times_out_a_single_operation() {
    let executor =
        DirectoryExecutor::with_operation_timeout(SlowDirectoryClient, Duration::from_millis(20));
    let plan =
        DirectoryPlan::try_from_batch(&directory_batch(13), &DomainDirectoryConfig::example())
            .unwrap();
    let context = DirectoryExecutionContext::try_from_batch(
        &directory_batch(13),
        &DomainDirectoryConfig::example(),
    )
    .unwrap();

    let result = executor.execute(&plan, &context).await.unwrap();

    assert_eq!(result.summary.failed, 1);
    let failure = result.failure.unwrap();
    assert_eq!(failure.operation, "ensure_ou");
    assert_eq!(failure.subject, "ou-rd");
    assert!(failure.detail.contains("timed out"));
}

#[test]
fn ldap_filter_values_escape_special_characters() {
    assert_eq!(
        escape_ldap_filter_value(r#"1001*(ops)\zero"#),
        r#"1001\2a\28ops\29\5czero"#
    );
    assert_eq!(escape_ldap_filter_value("张三"), "张三");
}

#[test]
fn ldap_dn_values_escape_rdn_special_characters() {
    assert_eq!(escape_ldap_dn_value("Dev,Ops+QA"), r#"Dev\,Ops\+QA"#);
    assert_eq!(escape_ldap_dn_value("#Root"), r#"\#Root"#);
    assert_eq!(escape_ldap_dn_value("Trailing "), r#"Trailing\ "#);
}

#[test]
fn ad_unicode_password_is_quoted_utf16_little_endian() {
    assert_eq!(
        encode_ad_unicode_password("P@ss"),
        vec![34, 0, 80, 0, 64, 0, 115, 0, 115, 0, 34, 0]
    );
}

#[test]
fn directory_execution_context_uses_precomputed_ou_dns() {
    let domain = DomainDirectoryConfig::example();
    let batch = DirectoryBatch {
        server_revision: 10,
        batch_revision: 10,
        organizational_units: vec![
            OrganizationalUnit {
                id: "child".to_string(),
                name: "研发二部".to_string(),
                parent_id: Some("parent".to_string()),
                changed_revision: 10,
            },
            OrganizationalUnit {
                id: "parent".to_string(),
                name: "研发部".to_string(),
                parent_id: None,
                changed_revision: 10,
            },
        ],
        organizational_unit_dns: std::collections::BTreeMap::from([
            (
                "child".to_string(),
                "OU=研发二部,OU=研发部,OU=Mirror,DC=example,DC=com".to_string(),
            ),
            (
                "parent".to_string(),
                "OU=研发部,OU=Mirror,DC=example,DC=com".to_string(),
            ),
        ]),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let context = DirectoryExecutionContext::try_from_batch(&batch, &domain).unwrap();

    assert_eq!(
        context.organizational_unit_dn("child").unwrap(),
        "OU=研发二部,OU=研发部,OU=Mirror,DC=example,DC=com"
    );
}

#[test]
fn directory_execution_context_does_not_require_parent_ou_in_the_batch() {
    let domain = DomainDirectoryConfig::example();
    let batch = DirectoryBatch {
        server_revision: 11,
        batch_revision: 11,
        organizational_units: vec![OrganizationalUnit {
            id: "child".to_string(),
            name: "研发二部".to_string(),
            parent_id: Some("parent".to_string()),
            changed_revision: 11,
        }],
        organizational_unit_dns: std::collections::BTreeMap::from([
            (
                "child".to_string(),
                "OU=研发二部,OU=研发部,OU=Mirror,DC=example,DC=com".to_string(),
            ),
            (
                "parent".to_string(),
                "OU=研发部,OU=Mirror,DC=example,DC=com".to_string(),
            ),
        ]),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let context = DirectoryExecutionContext::try_from_batch(&batch, &domain).unwrap();

    assert_eq!(
        context.organizational_unit_dn("child").unwrap(),
        "OU=研发二部,OU=研发部,OU=Mirror,DC=example,DC=com"
    );
}

#[test]
fn directory_execution_context_rejects_missing_precomputed_ou_dn() {
    let batch = DirectoryBatch {
        server_revision: 10,
        batch_revision: 10,
        organizational_units: vec![OrganizationalUnit {
            id: "child".to_string(),
            name: "研发二部".to_string(),
            parent_id: Some("missing-parent".to_string()),
            changed_revision: 10,
        }],
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: Vec::new(),
        groups: Vec::new(),
        has_more: false,
    };

    let context =
        DirectoryExecutionContext::try_from_batch(&batch, &DomainDirectoryConfig::example())
            .unwrap();
    let error = context.organizational_unit_dn("child").unwrap_err();

    assert!(error.to_string().contains("missing OU DN"));
}

#[derive(Clone)]
struct RecordingDirectoryClient {
    calls: Arc<Mutex<Vec<String>>>,
    opened_batches: Arc<Mutex<u32>>,
    fail_kind: Option<DirectoryOperationKind>,
    fail_password_employee_id: Option<&'static str>,
}

struct SlowDirectoryClient;
struct SlowDirectoryBatch;

#[async_trait]
impl DirectoryClient for SlowDirectoryClient {
    type Batch = SlowDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        Ok(SlowDirectoryBatch)
    }
}

#[async_trait]
impl DirectoryBatchSession for SlowDirectoryBatch {
    async fn apply(
        &mut self,
        _operation: &DirectoryOperation,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn set_password(
        &mut self,
        _credential: &CredentialEntry,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }
}

#[async_trait]
impl DirectoryClient for RecordingDirectoryClient {
    type Batch = RecordingDirectoryBatch;

    async fn open_batch(&self) -> anyhow::Result<Self::Batch> {
        *self.opened_batches.lock().unwrap() += 1;
        Ok(RecordingDirectoryBatch(self.clone()))
    }
}

struct RecordingDirectoryBatch(RecordingDirectoryClient);

#[async_trait]
impl DirectoryBatchSession for RecordingDirectoryBatch {
    async fn apply(
        &mut self,
        operation: &DirectoryOperation,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        self.0
            .calls
            .lock()
            .unwrap()
            .push(format!("{:?}", operation.kind));
        if self.0.fail_kind == Some(operation.kind) {
            anyhow::bail!("LDAP permission denied");
        }

        Ok(())
    }

    async fn set_password(
        &mut self,
        credential: &CredentialEntry,
        _context: &DirectoryExecutionContext,
    ) -> anyhow::Result<()> {
        self.0
            .calls
            .lock()
            .unwrap()
            .push(format!("password:{}", credential.employee_id));
        if self.0.fail_password_employee_id == Some(credential.employee_id.as_str()) {
            anyhow::bail!("password denied");
        }

        Ok(())
    }
}

fn directory_batch(batch_revision: u64) -> DirectoryBatch {
    DirectoryBatch {
        server_revision: batch_revision,
        batch_revision,
        organizational_units: vec![OrganizationalUnit {
            id: "ou-rd".to_string(),
            name: "Research".to_string(),
            parent_id: None,
            changed_revision: batch_revision,
        }],
        organizational_unit_dns: std::collections::BTreeMap::new(),
        users: vec![User {
            employee_id: "1001".to_string(),
            username: "zhangsan".to_string(),
            display_name: "Zhang San".to_string(),
            email: Some("zhangsan@example.com".to_string()),
            mobile: None,
            telephone: None,
            organizational_unit_id: "ou-rd".to_string(),
            status: UserStatus::Active,
            changed_revision: batch_revision,
        }],
        groups: vec![Group {
            id: "dev".to_string(),
            name: "Developers".to_string(),
            organizational_unit_id: "ou-rd".to_string(),
            member_employee_ids: vec!["1001".to_string()],
            changed_revision: batch_revision,
        }],
        has_more: false,
    }
}
