use rustflow_server::{
    testing::{TestMongoDatabase, TestSqlDatabase},
    DatabaseBackend, sqlx,
};
use std::env;

fn configured_url(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("{name} must be configured for database fixture tests in CI");
        }
        _ => None,
    }
}

async fn exercise_external_sql_fixture(url: &str) {
    let first = TestSqlDatabase::connect_test_url(url)
        .await
        .expect("connect to explicitly named test database");
    let second = TestSqlDatabase::connect_test_url(url)
        .await
        .expect("connect a second isolated fixture");

    let first_table = first
        .namespace()
        .table_name("fixture_records")
        .expect("safe generated SQL identifier");
    let second_table = second
        .namespace()
        .table_name("fixture_records")
        .expect("safe generated SQL identifier");
    assert_ne!(first_table, second_table);

    for (fixture, table, value) in [
        (&first, &first_table, "first fixture"),
        (&second, &second_table, "second fixture"),
    ] {
        fixture
            .pool()
            .execute_raw(
                &format!("CREATE TABLE {table} (value VARCHAR(128) NOT NULL)"),
                std::iter::empty(),
            )
            .await
            .expect("create fixture-owned table");

        let placeholder = match fixture.pool().backend() {
            DatabaseBackend::Postgres => "$1",
            DatabaseBackend::Sqlite | DatabaseBackend::MySql => "?",
        };
        fixture
            .pool()
            .execute_raw(
                &format!("INSERT INTO {table} (value) VALUES ({placeholder})"),
                [value.to_owned().into()],
            )
            .await
            .expect("insert fixture-owned row");

        let observed = match fixture.pool().backend() {
            DatabaseBackend::Postgres => {
                sqlx::query_scalar::<_, String>(&format!("SELECT value FROM {table}"))
                    .fetch_one(
                        fixture
                            .pool()
                            .sqlx_postgres_pool()
                            .expect("PostgreSQL pool"),
                    )
                    .await
                    .expect("read fixture-owned row")
            }
            DatabaseBackend::MySql => {
                sqlx::query_scalar::<_, String>(&format!("SELECT value FROM {table}"))
                    .fetch_one(fixture.pool().sqlx_mysql_pool().expect("MySQL pool"))
                    .await
                    .expect("read fixture-owned row")
            }
            DatabaseBackend::Sqlite => unreachable!("external SQL fixtures reject SQLite URLs"),
        };
        assert_eq!(observed, value);

        fixture
            .pool()
            .execute_raw(&format!("DROP TABLE {table}"), std::iter::empty())
            .await
            .expect("drop only fixture-owned table");
    }

    first.close().await;
    second.close().await;
}

#[tokio::test]
async fn sqlite_fixtures_are_independent_and_close_cleanly() {
    let first = TestSqlDatabase::sqlite_in_memory()
        .await
        .expect("first in-memory database");
    let second = TestSqlDatabase::sqlite_in_memory()
        .await
        .expect("second in-memory database");

    assert_ne!(
        first.namespace().table_name("items").unwrap(),
        second.namespace().table_name("items").unwrap()
    );

    sqlx::query("CREATE TABLE records (value TEXT NOT NULL)")
        .execute(first.pool().sqlx_sqlite_pool().expect("first SQLite pool"))
        .await
        .expect("create first in-memory table");
    let second_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'records'",
    )
    .fetch_one(second.pool().sqlx_sqlite_pool().expect("second SQLite pool"))
    .await
    .expect("inspect second in-memory database");
    assert_eq!(second_table_count, 0);

    first.close().await;
    second.close().await;
}

#[tokio::test]
async fn external_sql_fixtures_use_unique_table_names_and_cleanup() {
    let Some(postgres) = configured_url("RUSTFLOW_TEST_DATABASE_URL") else {
        eprintln!("Skipping PostgreSQL fixture contract outside CI: URL is unset");
        return;
    };
    let Some(mysql) = configured_url("RUSTFLOW_TEST_MYSQL_URL") else {
        eprintln!("Skipping MySQL fixture contract outside CI: URL is unset");
        return;
    };
    let Some(mariadb) = configured_url("RUSTFLOW_TEST_MARIADB_URL") else {
        eprintln!("Skipping MariaDB fixture contract outside CI: URL is unset");
        return;
    };

    for url in [&postgres, &mysql, &mariadb] {
        exercise_external_sql_fixture(url).await;
    }
}

#[tokio::test]
async fn mongodb_fixture_uses_and_drops_a_unique_database() {
    let Some(uri) = configured_url("RUSTFLOW_TEST_MONGODB_URI") else {
        eprintln!("Skipping MongoDB fixture contract outside CI: URI is unset");
        return;
    };

    let fixture = TestMongoDatabase::connect(uri)
        .await
        .expect("connect isolated MongoDB fixture");
    let database_name = fixture.namespace().database_name().to_owned();
    assert!(database_name.starts_with("rf_test_"));
    fixture.database().ping().await.expect("ping fixture database");

    fixture
        .database()
        .run_with_timeout(
            fixture
                .database()
                .database()
                .create_collection("fixture_probe"),
        )
        .await
        .expect("create a collection in the isolated database");

    fixture.cleanup().await.expect("drop isolated test database");
}
