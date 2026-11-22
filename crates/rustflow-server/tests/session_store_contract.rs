use std::env;

use rustflow_server::{
    DatabaseBackend, DatabaseConfig, DatabasePool, SessionError, SessionId, SessionStore,
    SqlSessionStore,
};

fn configured_url(variable: &str) -> Option<String> {
    match env::var(variable) {
        Ok(url) if !url.is_empty() => Some(url),
        Ok(_) | Err(_) if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("{variable} must be set to run session-store integration tests in CI");
        }
        _ => None,
    }
}

async fn run_session_store_contract(url: &str, expected_backend: DatabaseBackend) {
    let config = DatabaseConfig::new(url)
        .expect("valid database URL")
        .with_max_connections(4)
        .expect("valid pool size");
    assert_eq!(config.backend(), expected_backend);
    let database = DatabasePool::connect(&config)
        .await
        .expect("connect isolated database service");
    let store = SqlSessionStore::new(database.clone());
    store.ensure_schema().await.expect("ensure session schema");

    let old_id = SessionId::generate().expect("old ID");
    store
        .create(old_id.clone(), 200, 100)
        .await
        .expect("create");
    store
        .set(
            &old_id,
            "profile".to_owned(),
            serde_json::json!({"theme":"dark"}),
            120,
        )
        .await
        .expect("set value");
    assert_eq!(
        store.create(old_id.clone(), 201, 100).await,
        Err(SessionError::IdentifierCollision)
    );
    assert_eq!(
        store.get(&old_id, "profile", 199).await.expect("get value"),
        Some(serde_json::json!({"theme":"dark"}))
    );

    let new_id = SessionId::generate().expect("new ID");
    store
        .rotate(Some(&old_id), new_id.clone(), 300, 150, true)
        .await
        .expect("rotate");
    assert_eq!(store.expires_at(&old_id, 151).await.expect("old ID"), None);
    assert_eq!(
        store
            .get(&new_id, "profile", 151)
            .await
            .expect("copied value"),
        Some(serde_json::json!({"theme":"dark"}))
    );

    store.invalidate(&new_id).await.expect("invalidate");
    assert_eq!(
        store.expires_at(&new_id, 152).await.expect("revoked ID"),
        None
    );

    let expired_id = SessionId::generate().expect("expired ID");
    store
        .create(expired_id.clone(), 300, 200)
        .await
        .expect("create expiring session");
    assert_eq!(
        store.expires_at(&expired_id, 300).await.expect("expiry"),
        None
    );
    database.close().await;
}

#[tokio::test]
async fn postgres_session_store_contract() {
    let Some(url) = configured_url("RUSTFLOW_TEST_DATABASE_URL") else {
        eprintln!(
            "Skipping PostgreSQL session-store test outside CI: RUSTFLOW_TEST_DATABASE_URL is unset"
        );
        return;
    };
    run_session_store_contract(&url, DatabaseBackend::Postgres).await;
}

#[tokio::test]
async fn mysql_session_store_contract() {
    let Some(url) = configured_url("RUSTFLOW_TEST_MYSQL_URL") else {
        eprintln!("Skipping MySQL session-store test outside CI: RUSTFLOW_TEST_MYSQL_URL is unset");
        return;
    };
    run_session_store_contract(&url, DatabaseBackend::MySql).await;
}

#[tokio::test]
async fn mariadb_session_store_contract() {
    let Some(url) = configured_url("RUSTFLOW_TEST_MARIADB_URL") else {
        eprintln!(
            "Skipping MariaDB session-store test outside CI: RUSTFLOW_TEST_MARIADB_URL is unset"
        );
        return;
    };
    run_session_store_contract(&url, DatabaseBackend::MySql).await;
}
