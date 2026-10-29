# SQLx, pools, transactions, and migrations (Phase 6.5)

The database API is experimental and pre-1.0. RustFlow re-exports SQLx from rustflow_server::sqlx and provides a small pool/configuration/migration boundary. It is not an ORM: models remain ordinary Rust types, queries are explicit, and SQL dialect differences remain the application's responsibility.

## Supported engines

The adapter enables SQLite, PostgreSQL, and the SQLx MySQL protocol driver through `AnyPool`. Connection schemes are:

- SQLite: `sqlite://rustflow.db?mode=rwc` or `sqlite::memory:`.
- PostgreSQL: `postgres://user:password@host/database` or `postgresql://user:password@host/database`.
- MySQL: `mysql://user:password@host:3306/database`.
- MariaDB: the same `mysql://` URL and SQLx MySQL driver; `mariadb://` is not accepted by SQLx `AnyPool` driver selection.

MySQL and MariaDB are represented by `DatabaseBackend::MySql` because a `mysql://` URL selects the driver and does not identify the responding server product. CI is configured to exercise MySQL 8.0 and MariaDB 11.4 separately; other server versions and vendor-specific SQL features are not implied. MongoDB remains a separate, unimplemented document adapter. SQL syntax, placeholders, migrations, and value support remain engine-specific. Bind user values rather than interpolating request data into SQL.

## Configuration and connection pool

DatabaseConfig::from_app_config resolves RUSTFLOW_DATABASE_URL, RUSTFLOW_DATABASE_MAX_CONNECTIONS, RUSTFLOW_DATABASE_MIN_CONNECTIONS, and RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS using RustFlow's configuration precedence. Development/test defaults to a local SQLite file. Production requires the URL in the process environment; it never reads dotenv files. The URL is redacted in Debug output.

DatabaseConfig::new accepts explicit SQLite, PostgreSQL, and MySQL-protocol URLs. MySQL-family URLs are parsed during configuration; malformed URLs are rejected with a non-sensitive error, and credentials are omitted from `Debug`. Pool defaults remain maximum 10 connections, minimum 0, and a 10-second acquire timeout. Bounds are validated; tune them to the database connection limit and application replica count.

For example:

~~~rust
use rustflow_server::{AppConfig, DatabaseConfig, DatabasePool};

let app_config = AppConfig::load()?;
let config = DatabaseConfig::from_app_config(&app_config)?;
let database = DatabasePool::connect(&config).await?;
~~~

In request handlers, capture Arc<DatabasePool> explicitly. There is no global service locator.

## Parameterized queries and transaction helpers

Use SQLx's normal query API and bind every value that comes from outside the program. The example below uses SQLite placeholder syntax; adapt SQL text for PostgreSQL when necessary.

~~~rust
use rustflow_server::{DatabaseConfig, DatabasePool, sqlx};

let config = DatabaseConfig::new("sqlite://rustflow.db?mode=rwc")?
    .with_max_connections(8)?;
let database = DatabasePool::connect(&config).await?;

let mut transaction = database.begin_transaction().await?;
sqlx::query("INSERT INTO notes (title) VALUES (?)")
    .bind("A validated title")
    .execute(transaction.connection())
    .await?;
transaction.commit().await?;
~~~

A transaction is committed explicitly; call rollback() when the operation should be discarded. Dropping an uncommitted SQLx transaction requests a rollback. Do not hold a transaction while awaiting unrelated network calls or rendering a response.

DatabaseError::to_http_error maps row-not-found to 404, recognized unique-key violations to 409, pool timeouts/closures to 503 and all other database faults to a generic 500. Internal SQL, driver messages, and connection credentials are not copied into the response. Applications should log the underlying error through their own trusted logging policy, never the URL or bound secrets.

## Versioned migrations and rollback policy

The runner uses SQLx's _sqlx_migrations table, versions and checksums. It rejects changes to migrations that have already been applied.

A simple forward-only migration can be named 0001_create_notes.sql. A reversible migration uses matching 0002_add_status.up.sql and 0002_add_status.down.sql files. See the SQLite fixtures in crates/rustflow-server/tests/fixtures/sqlite_migrations/.

- Apply pending migrations explicitly with DatabasePool::run_migrations(path). Re-running it for an unchanged migration directory is safe.
- rollback_migrations_to(path, target_version) is explicit and only works when down migrations exist for the versions being reverted. It is never run automatically.
- In production, prefer a forward corrective migration over reverting a schema already observed by deployed application versions. Back up and rehearse restore/rollback procedures before a destructive operation.
- Run migrations as a single deployment step before rolling out multiple application replicas. Do not independently run an uncontrolled schema change from every replica's startup path.

The generated CLI scaffold applies migrations automatically only in development/test. In production run cargo run --bin migrate as a dedicated deployment step. Migrations generated by rustflow generate migration are templates and are not executed at generation time.


## Dialect-specific migrations

SQLx does not rewrite one SQL dialect into another. Keep dialect-specific migration files in separate directories; do not copy SQLite/PostgreSQL DDL into a MySQL-family directory without checking syntax and types.

Every MySQL/MariaDB migration directory must contain `.rustflow-dialect` with the value `mysql` (a final newline is allowed). The shared marker represents SQLx's MySQL-protocol driver family, not detection of the actual server product. A mismatched marker is rejected before applying or reverting migrations. Existing SQLite/PostgreSQL migration directories without a marker remain compatible; if they include a marker, it must be `sqlite` or `postgres` respectively.

Some MySQL-family DDL statements implicitly commit. Do not assume a failed migration is fully atomic because the SQLx runner reports a migration error. Test recovery against the target server, back up important schemas, and prefer corrective forward migrations in production.

## Running MySQL/MariaDB integration tests

The CI workflow provisions disposable MySQL 8.0 and MariaDB 11.4 services and runs separate contracts under Rust 1.85 and stable. To run these tests locally, create dedicated disposable test databases and set both URLs:

```sh
export RUSTFLOW_TEST_MYSQL_URL='mysql://user:password@127.0.0.1:3306/rustflow_test'
export RUSTFLOW_TEST_MARIADB_URL='mysql://user:password@127.0.0.1:3307/rustflow_test'
cargo test -p rustflow-server --test database_engines -- --test-threads=1
```

When running locally outside CI, a test skips if its corresponding URL is absent. In CI, a missing URL is a failure, so the job cannot pass without explicitly running both server contracts.

## Roadmap status

Subphase 6.6.1 is under review: the branch adds SQLx's MySQL-protocol feature and real-server integration contracts for MySQL 8.0 and MariaDB 11.4. It must not be considered accepted until the PR checks pass. Subphases 6.6.2/6.6.3 remain open, and MongoDB still requires its own document-oriented adapter. MongoDB must not be routed through SQLx `AnyPool`; its connection configuration, BSON mapping, indexes, and collection lifecycle need their own contract.

Phase 6.7 is planned to deliver a mature, cohesive typed SQL ORM for SQLite, PostgreSQL, MySQL, and MariaDB, plus a separate mature document-mapping/ODM API for MongoDB. This is not a CRUD-only MVP target. The subphases organize implementation work; they do not permit Phase 6.7 to close until model metadata/type mapping, a composable query builder, relations and explicit efficient loading, transaction/error semantics, derive diagnostics, driver-specific behavior, generated-app examples, and CI-backed integration coverage meet the full acceptance gate. An ADR must select the architectural pattern, dependency strategy, and mandatory/excluded feature matrix before the public contract is fixed. These capabilities are not implemented or stabilized yet.

## Testing

The server tests exercise repeatable SQLite migrations, forward application, rollback, parameterized writes, and—when configured—PostgreSQL pool/transaction behavior. The new `database_engines` integration target exercises MySQL and MariaDB separately, including URL redaction, real server identity, CRUD with bound values, transactions, duplicate-key mapping, migration failure/rollback, and pool closure. CI configures disposable PostgreSQL, MySQL, and MariaDB services. The MySQL-family cases skip when their URLs are absent outside CI and fail if a URL is absent in CI.

Run:

~~~sh
cargo test -p rustflow-server database::
cargo test -p rustflow-server --all-features
cargo test --workspace --all-features
~~~

This feature does not include an ORM, a general model/query abstraction, migration generation by sqlx-cli, a MongoDB adapter, database-backed sessions, or a built-in repository layer. MySQL/MariaDB support remains under review until the new integration jobs pass. The session contract remains separate; durable multi-instance session storage needs its own adapter and a security/consistency review.
