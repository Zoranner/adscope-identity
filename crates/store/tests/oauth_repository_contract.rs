use adss_protocol::UserStatus;
use adss_store::{
    AuthorizationCodeExchange, AuthorizationCodeRecord, OAuthClientRecord, OAuthClientType,
    Repository, UserDirectoryPatch,
};
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn oauth_client_crud_sorts_and_round_trips_both_client_types() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let repository = Repository::from_connection(db.clone());
    repository.initialize_schema().await.unwrap();
    let desktop = oauth_client("z-desktop", OAuthClientType::Desktop);
    let web = oauth_client("a-web", OAuthClientType::Web);

    assert_eq!(
        repository
            .create_oauth_client(desktop.clone())
            .await
            .unwrap(),
        Some(desktop.clone())
    );
    assert_eq!(
        repository.create_oauth_client(web.clone()).await.unwrap(),
        Some(web.clone())
    );
    assert!(
        repository
            .create_oauth_client(web.clone())
            .await
            .unwrap()
            .is_none()
    );

    let clients = repository.list_oauth_clients().await.unwrap();
    assert_eq!(clients, vec![web.clone(), desktop.clone()]);
    assert_eq!(
        repository.get_oauth_client("a-web").await.unwrap(),
        Some(web.clone())
    );

    let stored_types = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT client_type FROM oauth_clients ORDER BY client_id".to_string(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "client_type").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(stored_types, vec!["web", "desktop"]);

    let mut updated = web.clone();
    updated.name = "Updated web client".to_string();
    updated.client_secret_hash = Some("hash:updated".to_string());
    updated.redirect_uris = vec![
        "https://updated.example.com/callback".to_string(),
        "https://updated.example.com/secondary".to_string(),
    ];
    updated.allowed_scopes = vec!["openid".to_string(), "profile".to_string()];
    updated.enabled = false;
    assert_eq!(
        repository
            .update_oauth_client(updated.clone())
            .await
            .unwrap(),
        Some(updated.clone())
    );
    assert_eq!(
        repository
            .update_oauth_client(oauth_client("missing", OAuthClientType::Web))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repository.get_oauth_client("a-web").await.unwrap(),
        Some(updated)
    );

    assert!(!repository.delete_oauth_client("missing").await.unwrap());
    assert!(repository.delete_oauth_client("a-web").await.unwrap());
    assert!(!repository.delete_oauth_client("a-web").await.unwrap());
    assert!(
        repository
            .get_oauth_client("a-web")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn oauth_client_reads_reject_invalid_storage_and_malformed_json() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let repository = Repository::from_connection(db.clone());
    repository.initialize_schema().await.unwrap();
    repository
        .create_oauth_client(oauth_client("client", OAuthClientType::Web))
        .await
        .unwrap();

    db.execute_unprepared(
        "UPDATE oauth_clients SET client_type = 'Web' WHERE client_id = 'client'",
    )
    .await
    .unwrap();
    assert!(repository.get_oauth_client("client").await.is_err());

    db.execute_unprepared(
        "UPDATE oauth_clients SET client_type = 'web', redirect_uris = 'not-json' WHERE client_id = 'client'",
    )
    .await
    .unwrap();
    assert!(repository.get_oauth_client("client").await.is_err());

    db.execute_unprepared(
        "UPDATE oauth_clients SET redirect_uris = '[]', allowed_scopes = '{\"scope\":true}' WHERE client_id = 'client'",
    )
    .await
    .unwrap();
    assert!(repository.get_oauth_client("client").await.is_err());
}

#[tokio::test]
async fn authorization_code_is_consumed_once_and_expired_code_is_destroyed() {
    let repository = sqlite_repository().await;
    let active = authorization_code("active", 2_000);

    repository
        .store_authorization_code(active.clone())
        .await
        .unwrap();
    assert_eq!(
        repository
            .consume_authorization_code("active", 1_999)
            .await
            .unwrap(),
        Some(active)
    );
    assert!(
        repository
            .consume_authorization_code("active", 1_999)
            .await
            .unwrap()
            .is_none()
    );

    repository
        .store_authorization_code(authorization_code("expired", 2_000))
        .await
        .unwrap();
    assert!(
        repository
            .consume_authorization_code("expired", 2_000)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repository
            .consume_authorization_code("expired", 1_999)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn authorization_code_exchange_returns_bound_client_and_user_snapshot() {
    let repository = sqlite_repository().await;
    let mut client = oauth_client("client", OAuthClientType::Web);
    client.client_secret_hash = Some("secret-hash-that-must-not-leak".to_string());
    repository
        .create_oauth_client(client.clone())
        .await
        .unwrap();
    seed_user(&repository, "1001").await;
    let code = authorization_code("exchange", 2_000);
    repository
        .store_authorization_code(code.clone())
        .await
        .unwrap();

    let exchange: AuthorizationCodeExchange = repository
        .consume_authorization_code_for_exchange("exchange", 1_999)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(exchange.code, code);
    assert_eq!(exchange.client, Some(client));
    assert_eq!(exchange.user.as_ref().unwrap().employee_id, "1001");
    assert!(!format!("{exchange:?}").contains("secret-hash-that-must-not-leak"));
}

#[tokio::test]
async fn expired_authorization_code_exchange_destroys_code() {
    let repository = sqlite_repository().await;
    repository
        .store_authorization_code(authorization_code("expired-exchange", 2_000))
        .await
        .unwrap();

    assert!(
        repository
            .consume_authorization_code_for_exchange("expired-exchange", 2_000)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repository
            .consume_authorization_code_for_exchange("expired-exchange", 1_999)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn authorization_code_exchange_succeeds_only_once() {
    let repository = sqlite_repository().await;
    repository
        .store_authorization_code(authorization_code("repeat-exchange", 2_000))
        .await
        .unwrap();

    assert!(
        repository
            .consume_authorization_code_for_exchange("repeat-exchange", 1_999)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        repository
            .consume_authorization_code_for_exchange("repeat-exchange", 1_999)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn authorization_code_exchange_returns_missing_related_records_as_none() {
    let repository = sqlite_repository().await;
    repository
        .store_authorization_code(authorization_code("orphaned-exchange", 2_000))
        .await
        .unwrap();

    let exchange = repository
        .consume_authorization_code_for_exchange("orphaned-exchange", 1_999)
        .await
        .unwrap()
        .unwrap();

    assert!(exchange.client.is_none());
    assert!(exchange.user.is_none());
    assert!(
        repository
            .consume_authorization_code_for_exchange("orphaned-exchange", 1_999)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn authorization_code_exchange_conversion_errors_do_not_restore_code() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let repository = Repository::from_connection(db.clone());
    repository.initialize_schema().await.unwrap();
    repository
        .create_oauth_client(oauth_client("client", OAuthClientType::Web))
        .await
        .unwrap();
    seed_user(&repository, "1001").await;

    repository
        .store_authorization_code(authorization_code("invalid-code-json", 2_000))
        .await
        .unwrap();
    db.execute_unprepared(
        "UPDATE oauth_authorization_codes SET scopes = 'not-json' WHERE code_hash = 'invalid-code-json'",
    )
    .await
    .unwrap();
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-code-json", 1_999)
            .await
            .is_err()
    );
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-code-json", 1_999)
            .await
            .unwrap()
            .is_none()
    );

    repository
        .store_authorization_code(authorization_code("invalid-client-json", 2_000))
        .await
        .unwrap();
    db.execute_unprepared(
        "UPDATE oauth_clients SET redirect_uris = 'not-json' WHERE client_id = 'client'",
    )
    .await
    .unwrap();
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-client-json", 1_999)
            .await
            .is_err()
    );
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-client-json", 1_999)
            .await
            .unwrap()
            .is_none()
    );

    db.execute_unprepared(
        "UPDATE oauth_clients SET redirect_uris = '[]' WHERE client_id = 'client'",
    )
    .await
    .unwrap();
    repository
        .store_authorization_code(authorization_code("invalid-user-status", 2_000))
        .await
        .unwrap();
    db.execute_unprepared("UPDATE users SET status = 'invalid' WHERE employee_id = '1001'")
        .await
        .unwrap();
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-user-status", 1_999)
            .await
            .is_err()
    );
    assert!(
        repository
            .consume_authorization_code_for_exchange("invalid-user-status", 1_999)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn concurrent_authorization_code_consumers_have_exactly_one_winner() {
    let (database_url, database_path) = sqlite_file_database_url("oauth-concurrent-consume");
    let repository = Repository::connect(&database_url).await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository
        .store_authorization_code(authorization_code("single-use", 2_000))
        .await
        .unwrap();
    let first = Repository::connect(&database_url).await.unwrap();
    let second = Repository::connect(&database_url).await.unwrap();

    let (first_result, second_result) = tokio::join!(
        first.consume_authorization_code("single-use", 1_000),
        second.consume_authorization_code("single-use", 1_000),
    );
    let success_count = [first_result.unwrap(), second_result.unwrap()]
        .into_iter()
        .filter(Option::is_some)
        .count();

    assert_eq!(success_count, 1);
    drop(first);
    drop(second);
    drop(repository);
    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
async fn concurrent_authorization_code_exchanges_have_exactly_one_winner() {
    let (database_url, database_path) = sqlite_file_database_url("oauth-concurrent-exchange");
    let repository = Repository::connect(&database_url).await.unwrap();
    repository.initialize_schema().await.unwrap();
    repository
        .create_oauth_client(oauth_client("client", OAuthClientType::Web))
        .await
        .unwrap();
    seed_user(&repository, "1001").await;
    repository
        .store_authorization_code(authorization_code("single-exchange", 2_000))
        .await
        .unwrap();
    let first = Repository::connect(&database_url).await.unwrap();
    let second = Repository::connect(&database_url).await.unwrap();

    let (first_result, second_result) = tokio::join!(
        first.consume_authorization_code_for_exchange("single-exchange", 1_000),
        second.consume_authorization_code_for_exchange("single-exchange", 1_000),
    );
    let success_count = [first_result.unwrap(), second_result.unwrap()]
        .into_iter()
        .filter(Option::is_some)
        .count();

    assert_eq!(success_count, 1);
    drop(first);
    drop(second);
    drop(repository);
    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
async fn expired_authorization_code_cleanup_honors_limit_and_zero_is_noop() {
    let repository = sqlite_repository().await;
    for (code_hash, expires_at) in [("one", 1), ("two", 2), ("three", 3), ("future", 100)] {
        repository
            .store_authorization_code(authorization_code(code_hash, expires_at))
            .await
            .unwrap();
    }

    assert_eq!(
        repository
            .delete_expired_authorization_codes(50, 0_u64)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        repository
            .delete_expired_authorization_codes(50, 2_u64)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        repository
            .delete_expired_authorization_codes(50, 10_u64)
            .await
            .unwrap(),
        1
    );
    assert!(
        repository
            .consume_authorization_code("future", 50)
            .await
            .unwrap()
            .is_some()
    );
}

fn oauth_client(client_id: &str, client_type: OAuthClientType) -> OAuthClientRecord {
    OAuthClientRecord {
        client_id: client_id.to_string(),
        name: format!("{client_id} name"),
        client_type,
        client_secret_hash: None,
        redirect_uris: vec![format!("https://{client_id}.example.com/callback")],
        allowed_scopes: vec!["openid".to_string(), "email".to_string()],
        enabled: true,
    }
}

fn authorization_code(code_hash: &str, expires_at: i64) -> AuthorizationCodeRecord {
    AuthorizationCodeRecord {
        code_hash: code_hash.to_string(),
        client_id: "client".to_string(),
        employee_id: "1001".to_string(),
        redirect_uri: "https://client.example.com/callback".to_string(),
        scopes: vec!["openid".to_string(), "profile".to_string()],
        nonce: "nonce-1".to_string(),
        code_challenge: "challenge-1".to_string(),
        auth_time: 1_000,
        expires_at,
    }
}

async fn seed_user(repository: &Repository, employee_id: &str) {
    repository
        .upsert_directory(
            Vec::new(),
            vec![UserDirectoryPatch {
                employee_id: employee_id.to_string(),
                username: format!("user-{employee_id}"),
                display_name: format!("User {employee_id}"),
                email: Some(format!("user-{employee_id}@example.com")),
                mobile: None,
                telephone: None,
                organizational_unit_id: "ou-rd".to_string(),
                status: UserStatus::Active,
            }],
            Vec::new(),
        )
        .await
        .unwrap();
}

async fn sqlite_repository() -> Repository {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
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
