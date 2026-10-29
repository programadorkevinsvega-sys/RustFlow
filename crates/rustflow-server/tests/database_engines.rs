use std::{
    env, fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use http::StatusCode;
use rustflow_server::{DatabaseBackend, DatabaseConfig, DatabaseError, DatabasePool};

#[derive(Clone, Copy)]
enum ExpectedServer {
    MySql,
    MariaDb,
}

impl ExpectedServer {
    fn url_variable(self) -> &'static str {
        match self {
            Self::MySql => "RUSTFLOW_TEST_MYSQL_URL",
            Self::MariaDb => "RUSTFLOW_TEST_MARIADB_URL",
        }
    }

    fn migration_directory(self) -> &'static str {
        match self {
            Self::MySql => "mysql_migrations",
            Self::MariaDb => "mariadb_migrations",
        }
    }

    fn is_mariadb(self) -> bool {
        matches!(self, Self::MariaDb)
    }
}

fn configured_url(server: ExpectedServer) -> Option<String> {
    match env::var(server.url_variable()) {
        Ok(url) if !url.is_empty() => Some(url),
        Ok(_) if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!(
                "{} must be set to run database integration tests in CI",
                server.url_variable()
            );
        }
        Err(_) if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!(
                "{} must be set to run database integration tests in CI",
                server.url_variable()
            );
        }
        _ => None,
    }
}

fn fixtures(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

async fn run_contract(url: &str, server: ExpectedServer) {
    let config = DatabaseConfig::new(url)
        .expect("validated MySQL/MariaDB URL")
        .with_max_connections(2)
        .expect("valid pool size");
    assert_eq!(config.backend(), DatabaseBackend::MySql);
    assert!(
        !format!("{config:?}").contains(url),
        "Debug must redact the URL"
    );

    // Check an unavailable endpoint and rejected credentials without exposing either URL.
    let unavailable =
        DatabaseConfig::new("mysql://rustflow_invalid:invalid-password@127.0.0.1:1/rustflow_test")
            .expect("valid-shaped MySQL URL")
            .with_acquire_timeout(Duration::from_secs(2))
            .expect("positive timeout");
    assert!(
        tokio::time::timeout(Duration::from_secs(3), DatabasePool::connect(&unavailable))
            .await
            .expect("unavailable endpoint timeout bound")
            .is_err(),
        "connection to a closed local port must fail"
    );

    let authority_and_database = url.strip_prefix("mysql://").expect("mysql URL scheme");
    let (authority, database_and_options) = authority_and_database
        .split_once('/')
        .expect("database path in integration URL");
    let host_and_port = authority.rsplit('@').next().expect("host and port");
    let bad_credentials =
        format!("mysql://rustflow_invalid:invalid-password@{host_and_port}/{database_and_options}");
    let bad_config = DatabaseConfig::new(bad_credentials)
        .expect("well-formed URL with deliberately invalid credentials")
        .with_acquire_timeout(Duration::from_secs(3))
        .expect("positive timeout");
    let connect_error = DatabasePool::connect(&bad_config)
        .await
        .expect_err("invalid credentials must not connect");
    assert!(!connect_error.to_string().contains("invalid-password"));

    let database = DatabasePool::connect(&config)
        .await
        .expect("connect to configured database service");
    assert!(database.size() <= database.max_connections());

    let server_version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(database.pool())
        .await
        .expect("read server version");
    if server.is_mariadb() {
        assert!(
            server_version.to_ascii_lowercase().contains("mariadb"),
            "MariaDB job connected to unexpected server version: {server_version}"
        );
    } else {
        assert!(
            !server_version.to_ascii_lowercase().contains("mariadb"),
            "MySQL job must exercise MySQL, received: {server_version}"
        );
    }

    // CI gives each engine its own disposable database. Reset the fixture state so a
    // developer can repeat the same test against a dedicated local test database.
    sqlx::query("DROP TABLE IF EXISTS rustflow_phase661_items")
        .execute(database.pool())
        .await
        .expect("reset test table");
    sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
        .execute(database.pool())
        .await
        .expect("reset migration metadata");

    let migrations = fixtures(server.migration_directory());
    database
        .run_migrations(&migrations)
        .await
        .expect("first migration run");
    database
        .run_migrations(&migrations)
        .await
        .expect("repeat migration run");

    let injection_like_value = "name'); DROP TABLE rustflow_phase661_items; --";
    sqlx::query("INSERT INTO rustflow_phase661_items (value) VALUES (?)")
        .bind(injection_like_value)
        .execute(database.pool())
        .await
        .expect("parameterized insert");
    let selected: String =
        sqlx::query_scalar("SELECT value FROM rustflow_phase661_items WHERE value = ?")
            .bind(injection_like_value)
            .fetch_one(database.pool())
            .await
            .expect("read bound value");
    assert_eq!(selected, injection_like_value);

    sqlx::query("UPDATE rustflow_phase661_items SET value = ? WHERE value = ?")
        .bind("updated")
        .bind(injection_like_value)
        .execute(database.pool())
        .await
        .expect("parameterized update");
    let duplicate_error = sqlx::query("INSERT INTO rustflow_phase661_items (value) VALUES (?)")
        .bind("updated")
        .execute(database.pool())
        .await
        .expect_err("unique constraint must reject duplicate");
    let mapped_error: DatabaseError = duplicate_error.into();
    assert_eq!(
        mapped_error.to_http_error().status_code(),
        StatusCode::CONFLICT
    );
    assert!(std::error::Error::source(&mapped_error).is_some());

    let mut rollback = database
        .begin_transaction()
        .await
        .expect("begin rollback transaction");
    sqlx::query("INSERT INTO rustflow_phase661_items (value) VALUES (?)")
        .bind("rolled-back")
        .execute(rollback.connection())
        .await
        .expect("write in rollback transaction");
    rollback.rollback().await.expect("rollback transaction");

    let mut commit = database
        .begin_transaction()
        .await
        .expect("begin commit transaction");
    sqlx::query("INSERT INTO rustflow_phase661_items (value) VALUES (?)")
        .bind("committed")
        .execute(commit.connection())
        .await
        .expect("write in commit transaction");
    commit.commit().await.expect("commit transaction");

    let rolled_back_count: i64 =
        sqlx::query_scalar("SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items WHERE value = ?")
            .bind("rolled-back")
            .fetch_one(database.pool())
            .await
            .expect("count rolled back rows");
    assert_eq!(rolled_back_count, 0);
    let committed_count: i64 =
        sqlx::query_scalar("SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items WHERE value = ?")
            .bind("committed")
            .fetch_one(database.pool())
            .await
            .expect("count committed rows");
    assert_eq!(committed_count, 1);

    let deleted = sqlx::query("DELETE FROM rustflow_phase661_items WHERE value = ?")
        .bind("updated")
        .execute(database.pool())
        .await
        .expect("parameterized delete");
    assert_eq!(deleted.rows_affected(), 1);

    database
        .rollback_migrations_to(&migrations, 0)
        .await
        .expect("revert migration using down SQL");
    let table_count: i64 = sqlx::query_scalar(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.tables
         WHERE table_schema = DATABASE() AND table_name = 'rustflow_phase661_items'",
    )
    .fetch_one(database.pool())
    .await
    .expect("check rolled-back table");
    assert_eq!(table_count, 0);
    database
        .run_migrations(&migrations)
        .await
        .expect("reapply migration");
    let empty_count: i64 = sqlx::query_scalar("SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items")
        .fetch_one(database.pool())
        .await
        .expect("count after reapply");
    assert_eq!(empty_count, 0);

    let failed_dir = env::temp_dir().join(format!(
        "rustflow-phase661-migration-failure-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&failed_dir).expect("create failing migration directory");
    fs::write(failed_dir.join(".rustflow-dialect"), "mysql\n").expect("write dialect marker");
    for name in [
        "0001_create_phase661_items.up.sql",
        "0001_create_phase661_items.down.sql",
    ] {
        fs::copy(migrations.join(name), failed_dir.join(name))
            .expect("copy already-applied migration to preserve checksum");
    }
    fs::write(
        failed_dir.join("9999_intentionally_invalid.sql"),
        "THIS IS NOT VALID SQL;\n",
    )
    .expect("write intentionally invalid migration");
    let failed_migration = database.run_migrations(&failed_dir).await;
    let _ = fs::remove_dir_all(&failed_dir);
    assert!(failed_migration.is_err(), "invalid migration must fail");

    database.close().await;
    let after_close = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(database.pool())
        .await;
    assert!(after_close.is_err(), "closed pool must reject new work");
}

#[tokio::test]
async fn mysql_80_support_contract() {
    let Some(url) = configured_url(ExpectedServer::MySql) else {
        eprintln!("Skipping MySQL integration test outside CI: RUSTFLOW_TEST_MYSQL_URL is unset");
        return;
    };
    run_contract(&url, ExpectedServer::MySql).await;
}

#[tokio::test]
async fn mariadb_114_support_contract() {
    let Some(url) = configured_url(ExpectedServer::MariaDb) else {
        eprintln!(
            "Skipping MariaDB integration test outside CI: RUSTFLOW_TEST_MARIADB_URL is unset"
        );
        return;
    };
    run_contract(&url, ExpectedServer::MariaDb).await;
}
