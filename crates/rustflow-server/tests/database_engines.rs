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
        .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await
        .expect("read server version");
    if server.is_mariadb() {
        assert!(
            server_version.starts_with("11.4.13-MariaDB"),
            "MariaDB support contract expects 11.4.13, received: {server_version}"
        );
    } else {
        assert!(
            server_version.starts_with("8.4.13"),
            "MySQL support contract expects 8.4.13 LTS, received: {server_version}"
        );
    }
    println!("verified database service version: {server_version}");

    // CI gives each engine its own disposable database. Reset the fixture state so a
    // developer can repeat the same test against a dedicated local test database.
    sqlx::query("DROP TABLE IF EXISTS rustflow_phase661_items")
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await
        .expect("reset test table");
    sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
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
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await
        .expect("parameterized insert");
    let selected: String =
        sqlx::query_scalar("SELECT value FROM rustflow_phase661_items WHERE value = ?")
            .bind(injection_like_value)
            .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
            .await
            .expect("read bound value");
    assert_eq!(selected, injection_like_value);

    sqlx::query("UPDATE rustflow_phase661_items SET value = ? WHERE value = ?")
        .bind("updated")
        .bind(injection_like_value)
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await
        .expect("parameterized update");
    let duplicate_error = sqlx::query("INSERT INTO rustflow_phase661_items (value) VALUES (?)")
        .bind("updated")
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await
        .expect_err("unique constraint must reject duplicate");
    let mapped_error: DatabaseError = duplicate_error.into();
    assert_eq!(
        mapped_error.to_http_error().status_code(),
        StatusCode::CONFLICT
    );
    assert!(std::error::Error::source(&mapped_error).is_some());

    let rollback = database
        .begin_transaction()
        .await
        .expect("begin rollback transaction");
    rollback
        .execute_raw(
            "INSERT INTO rustflow_phase661_items (value) VALUES (?)",
            ["rolled-back".into()],
        )
        .await
        .expect("write in rollback transaction");
    rollback.rollback().await.expect("rollback transaction");

    let commit = database
        .begin_transaction()
        .await
        .expect("begin commit transaction");
    commit
        .execute_raw(
            "INSERT INTO rustflow_phase661_items (value) VALUES (?)",
            ["committed".into()],
        )
        .await
        .expect("write in commit transaction");
    commit.commit().await.expect("commit transaction");

    let rolled_back_count: i64 = sqlx::query_scalar(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items WHERE value = ?",
    )
    .bind("rolled-back")
    .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
    .await
    .expect("count rolled back rows");
    assert_eq!(rolled_back_count, 0);
    let committed_count: i64 = sqlx::query_scalar(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items WHERE value = ?",
    )
    .bind("committed")
    .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
    .await
    .expect("count committed rows");
    assert_eq!(committed_count, 1);

    let deleted = sqlx::query("DELETE FROM rustflow_phase661_items WHERE value = ?")
        .bind("updated")
        .execute(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
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
    .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
    .await
    .expect("check rolled-back table");
    assert_eq!(table_count, 0);
    database
        .run_migrations(&migrations)
        .await
        .expect("reapply migration");
    let empty_count: i64 =
        sqlx::query_scalar("SELECT CAST(COUNT(*) AS SIGNED) FROM rustflow_phase661_items")
            .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
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
        .fetch_one(database.sqlx_mysql_pool().expect("MySQL/MariaDB pool"))
        .await;
    assert!(after_close.is_err(), "closed pool must reject new work");
}

#[tokio::test]
#[ignore = "Run serially after the workspace suite to isolate the PostgreSQL migration ledger"]
async fn postgres_migrations_are_repeatable_and_reversible() {
    let url = match env::var("RUSTFLOW_TEST_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("RUSTFLOW_TEST_DATABASE_URL must be set for the PostgreSQL migration contract");
        }
        _ => return,
    };
    let config = DatabaseConfig::new(url)
        .expect("valid PostgreSQL test URL")
        .with_max_connections(2)
        .expect("valid PostgreSQL pool bounds");
    let database = DatabasePool::connect(&config)
        .await
        .expect("connect to PostgreSQL migration service");
    let version: String = rustflow_server::sqlx::query_scalar("SHOW server_version")
        .fetch_one(
            database
                .sqlx_postgres_pool()
                .expect("PostgreSQL typed pool"),
        )
        .await
        .expect("read PostgreSQL version");
    assert!(
        version.starts_with("16.15"),
        "Phase 6.6 support contract expects PostgreSQL 16.15, received {version}"
    );

    let pool = database
        .sqlx_postgres_pool()
        .expect("PostgreSQL typed pool");
    rustflow_server::sqlx::query("DROP TABLE IF EXISTS rustflow_phase676_pg_items")
        .execute(pool)
        .await
        .expect("reset PostgreSQL migration fixture");
    // This dedicated post-workspace step owns the CI database while testing the
    // SQLx migration ledger, so remove any ledger written by the generated-app test.
    rustflow_server::sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
        .execute(pool)
        .await
        .expect("reset dedicated PostgreSQL migration ledger");

    let migrations = fixtures("postgres_migrations");
    database
        .run_migrations(&migrations)
        .await
        .expect("first PostgreSQL migration run");
    database
        .run_migrations(&migrations)
        .await
        .expect("repeating PostgreSQL migrations must be a no-op");

    rustflow_server::sqlx::query("INSERT INTO rustflow_phase676_pg_items (value) VALUES ($1)")
        .bind("reversible-migration")
        .execute(pool)
        .await
        .expect("insert into migrated PostgreSQL table");
    let before_rollback: i64 =
        rustflow_server::sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_phase676_pg_items")
            .fetch_one(pool)
            .await
            .expect("count PostgreSQL migration rows");
    assert_eq!(before_rollback, 1);

    database
        .rollback_migrations_to(&migrations, 0)
        .await
        .expect("revert PostgreSQL migration using down SQL");
    database
        .run_migrations(&migrations)
        .await
        .expect("reapply PostgreSQL migration after rollback");
    let after_reapply: i64 =
        rustflow_server::sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_phase676_pg_items")
            .fetch_one(pool)
            .await
            .expect("count rows after PostgreSQL migration reapply");
    assert_eq!(after_reapply, 0);

    database.close().await;
}

#[tokio::test]
async fn mysql_8413_support_contract() {
    let Some(url) = configured_url(ExpectedServer::MySql) else {
        eprintln!("Skipping MySQL integration test outside CI: RUSTFLOW_TEST_MYSQL_URL is unset");
        return;
    };
    run_contract(&url, ExpectedServer::MySql).await;
}

#[tokio::test]
async fn mariadb_11413_support_contract() {
    let Some(url) = configured_url(ExpectedServer::MariaDb) else {
        eprintln!(
            "Skipping MariaDB integration test outside CI: RUSTFLOW_TEST_MARIADB_URL is unset"
        );
        return;
    };
    run_contract(&url, ExpectedServer::MariaDb).await;
}
