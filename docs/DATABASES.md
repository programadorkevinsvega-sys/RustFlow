# SQL pools, SeaORM, transactions, and migrations (Phase 6.7.2)

The database API remains experimental and pre-1.0. Phase 6.7.1 selected SeaORM 1.1.20 and defined the Data Mapper/model contract. Phase 6.7.2 is integrating SeaORM into `rustflow-server` while replacing the old SQLx `typed SQLx pool` boundary with backend-specific SQLx pools. The implementation is in progress: the connection bridge and typed accessors are present in the phase branch, while complete CRUD, relationship-loading, type-codec and four-engine ORM evidence remain acceptance work. MongoDB continues to use its independent document adapter.

## Supported engines

The adapter uses one backend-specific SQLx pool per configured database: `SqlitePool`, `PgPool`, or `MySqlPool`. MariaDB shares the MySQL protocol driver. SeaORM receives a clone of that same pool handle; it does not open a second pool. Connection schemes are:

- SQLite: `sqlite://rustflow.db?mode=rwc` or `sqlite::memory:`.
- PostgreSQL: `postgres://user:password@host/database` or `postgresql://user:password@host/database`.
- MySQL: `mysql://user:password@host:3306/database`.
- MariaDB: the same `mysql://` URL and SQLx MySQL driver; `mariadb://` is not accepted by SQLx `typed SQLx pool` driver selection.

MySQL and MariaDB are represented by `DatabaseBackend::MySql` because a `mysql://` URL selects the driver and does not identify the responding server product. The current pinned CI contract is PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS, SQLite 3.46.0 bundled by the locked SQLx / `libsqlite3-sys` dependencies, and MongoDB 7.0.45. The full tested version/MSRV matrix and evidence boundaries live in [DATABASE_SUPPORT_MATRIX.md](DATABASE_SUPPORT_MATRIX.md). These exact targets do not imply compatibility with every version or vendor-specific feature. MongoDB is implemented through a separate document adapter and does not use SQLx `typed SQLx pool` or SQL migrations. SQL syntax, placeholders, migrations, and value support remain engine-specific. Bind user values rather than interpolating request data into SQL.

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

## Typed pools, ORM access, and raw SQL

`DatabasePool::connect` creates one typed SQLx pool for the configured backend and constructs the SeaORM `DatabaseConnection` from a clone of that exact pool handle. SQLx and SeaORM therefore share the underlying connection pool, limits, acquire timeout and close lifecycle.

Use `DatabasePool::orm()` when calling SeaORM Entity, ActiveModel, query-builder and relation-loader APIs. The server re-exports SeaORM as `rustflow_server::sea_orm`; this keeps the ORM dependency inside `rustflow-server` and outside the browser/WASM core.

~~~rust
use rustflow_server::{AppConfig, DatabaseConfig, DatabasePool, sea_orm};

let app = AppConfig::load()?;
let config = DatabaseConfig::from_app_config(&app)?;
let database = DatabasePool::connect(&config).await?;

// Pass the shared connection explicitly to SeaORM operations.
let connection = database.orm();
~~~

Direct SQLx access remains available through backend-checked accessors. They return `None` when the configured backend does not match; they never convert or duplicate a pool.

~~~rust
use rustflow_server::{DatabaseConfig, DatabasePool, sqlx};

let config = DatabaseConfig::new("sqlite://rustflow.db?mode=rwc")?;
let database = DatabasePool::connect(&config).await?;
let pool = database.sqlx_sqlite_pool().expect("configured SQLite backend");

let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notes")
    .fetch_one(pool)
    .await?;
~~~

The available accessors are `sqlx_sqlite_pool()`, `sqlx_postgres_pool()`, and `sqlx_mysql_pool()`; the last is used for both MySQL and MariaDB. Direct SQLx queries remain backend-specific: placeholders, codecs and SQL syntax are not translated.

### Numeric codecs and decimal precision

SeaORM 1.1.20 with SeaQuery uses an SQLite-specific f64 path for both Decimal and BigDecimal: values are bound as floating point and read back through f64. The SQLite fixture therefore uses REAL columns and intentionally tests only a value that round-trips through that representation. This is not arbitrary-precision decimal storage; do not use SQLite Decimal/BigDecimal for money or other precision-sensitive values. Use scaled integers or a separately designed canonical-text codec when exact decimal semantics are required on SQLite. PostgreSQL uses NUMERIC, while MySQL and MariaDB use DECIMAL; their integration contracts exercise a high-precision BigDecimal round-trip.

For portable parameterized SQL through the SeaORM connection, use `query_raw` and `execute_raw`. Use `?` placeholders with SQLite/MySQL/MariaDB and numbered `$1`, `$2`, ... placeholders with PostgreSQL. Values must be passed as SeaORM `Value`s, not interpolated into SQL.

~~~rust
let rows = database
    .query_raw("SELECT title FROM notes WHERE id = ?", [requested_id.into()])
    .await?;
~~~

## Shared transactions

`DatabasePool::begin_transaction()` starts a SeaORM transaction on the same pool. The transaction exposes SeaORM's `ConnectionTrait`, so ORM operations and raw parameterized statements can run through the same transaction. It is deliberately no longer an SQLx `SeaORM transaction connection`.

~~~rust
let transaction = database.begin_transaction().await?;

transaction
    .execute_raw(
        "INSERT INTO notes (title) VALUES (?)",
        ["A validated title".into()],
    )
    .await?;

// SeaORM operations receive transaction.connection() explicitly.
transaction.commit().await?;
~~~

Call `rollback()` when the operation should be discarded. Do not hold a transaction while awaiting unrelated network calls or rendering a response. A connection loss during commit can leave the server outcome unknown; do not report it as a guaranteed rollback. For SQLx-only transactions, use the matching typed pool directly; do not start a second transaction when work must be atomic with ORM operations.

## Error mapping

`DatabaseError::to_http_error` maps row-not-found to 404, recognized unique-key violations to 409, pool timeouts/closures to 503 and other database faults to a generic 500. SQLx and SeaORM error sources remain available to trusted server-side diagnostics, while public messages do not expose SQL, credentials or bound secrets.

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

## MongoDB document adapter (Phase 6.6.2)

MongoDB is document-oriented and uses its own adapter, independent of SQLx, typed SQLx pool, SQL transactions, and SQL migrations. The current API is experimental and exposes the official driver's native collection API rather than an ODM.

The official MongoDB Rust driver is pinned to =3.6.0 because that release retains the repository's Rust 1.85 MSRV. The DNS SRV resolver feature is intentionally disabled because its Hickory 0.25 dependency triggered a high-severity dependency-review advisory; only standard mongodb:// URIs are supported by this initial adapter. Re-evaluate the pin and optional features during dependency/security reviews. The initial CI server contract is MongoDB 7.0.45.

### Configuration

Required settings are RUSTFLOW_MONGODB_URI and RUSTFLOW_MONGODB_DATABASE. Optional settings, expressed in positive integer seconds, are RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS, RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS, and RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS. Defaults are 10 seconds for connection, 10 seconds for server selection, and 15 seconds for operations passed through MongoDatabase::run_with_timeout.

MongoConfig::from_app_config follows AppConfig's existing precedence. In production, the URI and database name must come from process configuration; the adapter does not load dotenv files itself. The URI is redacted in Debug, and adapter error displays/public HTTP responses omit driver messages and credentials.

~~~rust
use rustflow_server::{AppConfig, MongoConfig, MongoDatabase};

let app = AppConfig::load()?;
let config = MongoConfig::from_app_config(&app)?;
let database = MongoDatabase::connect(&config).await?;
database.ping().await?;
~~~

### BSON, native CRUD, and indexes

Use mongodb_driver::bson and MongoDatabase::collection<T> to work with the driver's native query/update operators and serde-compatible document types. Wrap network actions in run_with_timeout when they need RustFlow's configured operation bound.

~~~rust
use rustflow_server::{
    DocumentBootstrap, DocumentIndex, MongoDatabase, mongodb_driver::bson::{doc, Document},
};

let notes = database.collection::<Document>("notes");
database
    .run_with_timeout(notes.insert_one(doc! { "title": "A note", "metadata": { "pinned": false } }))
    .await?;

let bootstrap = DocumentBootstrap::new("notes")?
    .with_index(DocumentIndex::new("title_idx", doc! { "title": 1 }, false)?)?;
database.bootstrap(&bootstrap).await?;
~~~

The bootstrap contract creates a missing collection, creates declared indexes (including explicitly unique indexes), and verifies declared index names. Re-running an unchanged declaration is intended to be idempotent. It does not drop resources, replace indexes, validate arbitrary document schemas, or implement versioned document migrations. A unique-index violation is mapped to HTTP 409 only when the MongoDB driver reports duplicate-key code 11000, including native write errors; other internal driver errors are not exposed in client-facing responses.

The adapter does not add an ODM, model derives, a portable query builder, SQL-like relations, or transaction/migration abstractions. Those capabilities are outside Phase 6.6.2 and belong to the separate Phase 6.7 document-mapper/ODM contract.

## Roadmap status

Phase 6.6 is complete: its SQL and MongoDB support contracts, runtime versions and evidence are recorded in `docs/DATABASE_SUPPORT_MATRIX.md`, ADR 0019 and `ROADMAP.md`. Phase 6.7.1 is complete and merged in PR #58 (`36c7d43652d41b3bf92e60cc3ad02405bbfdb1a1`); ADR 0020 selects Data Mapper with SeaORM 1.1.20 and the validation-only `ModelDiagnostics` derive. Phase 6.7.2 is now in progress on the typed-pool bridge. The ORM phase will not close until model CRUD, query composition, relations and explicit efficient loading, transaction/error semantics, cross-engine type mapping, generated-app examples and CI-backed integration coverage meet the accepted contract. MongoDB remains separate and is planned for Phase 6.7.3.

## Testing

The server tests exercise SQLite 3.46.0 migrations, forward application, rollback, parameterized writes, and PostgreSQL 16.15 pool/transaction behavior. The `database_engines` integration target exercises MySQL 8.4.13 and MariaDB 11.4.13 separately, including URL redaction, exact server version and identity, CRUD with bound values, transactions, duplicate-key mapping, migration failure/rollback, and pool closure. CI also provisions MongoDB 7.0.45 and explicitly runs the `mongodb_adapter` integration target. These service contracts are run on Rust 1.85 and stable; the corresponding tests fail in CI if a required service URL is absent.

Run:

~~~sh
cargo test -p rustflow-server database::
cargo test -p rustflow-server --all-features
cargo test --workspace --all-features
~~~

The current 6.7.2 branch introduces the SeaORM connection facade and shared typed-pool bridge, but the complete persistence contract is not yet closed: CRUD behavior, relationship loaders, checked type codecs and four-engine ORM integration coverage still need implementation and evidence. See [ADR 0020](adr/0020-orm-model-contract.md) and [the model/type contract](ORM_MODEL_CONTRACT.md). MongoDB has its own separate experimental adapter contract, merged in PR #55 as `e3ceb86d757bb7aa1a07e16a1e21b462adb69841`. MySQL/MariaDB support was introduced in PR #54 as `48a491d660c71005b58ed88a4f6a7e8ba3390c9d`; Phase 6.6.3 now retests the maintained MySQL 8.4.13 LTS and MariaDB 11.4.13 LTS lines alongside PostgreSQL 16.15, SQLite 3.46.0, and MongoDB 7.0.45. The session contract remains separate; durable multi-instance session storage needs its own adapter and a security/consistency review.
