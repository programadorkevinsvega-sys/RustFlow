# SQL pools, SeaORM, transactions, and migrations (Phase 6.7.2)

The database API remains experimental and pre-1.0. Phase 6.7.1 selected SeaORM 1.1.20 and defined the Data Mapper/model contract. The Phase 6.7.2 branch integrates SeaORM into `rustflow-server` over one backend-specific SQLx pool (SQLite, PostgreSQL or MySQL protocol) shared by SQLx and SeaORM. It exposes `EntityRepository` CRUD/bulk operations, typed SeaORM query composition, explicit relation loaders, parameterized raw SQL and mixed ORM/SQL transactions. Four-engine integration contracts cover SQLite, PostgreSQL, MySQL and MariaDB; acceptance remains pending the exact final PR SHA's full CI and the edge cases called out in the model contract. MongoDB continues to use its independent document adapter.

## Supported engines

The adapter uses one backend-specific SQLx pool per configured database: `SqlitePool`, `PgPool`, or `MySqlPool`. MariaDB shares the MySQL protocol driver. SeaORM receives a clone of that same pool handle; it does not open a second pool. Connection schemes are:

- SQLite: `sqlite://rustflow.db?mode=rwc` or `sqlite::memory:`.
- PostgreSQL: `postgres://user:password@host/database` or `postgresql://user:password@host/database`.
- MySQL: `mysql://user:password@host:3306/database`.
- MariaDB: the same `mysql://` URL and SQLx MySQL driver; `mariadb://` is not accepted by SQLx's MySQL driver URL parser.

MySQL and MariaDB are represented by `DatabaseBackend::MySql` because a `mysql://` URL selects the driver and does not identify the responding server product. The current pinned CI contract is PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS, SQLite 3.46.0 bundled by the locked SQLx / `libsqlite3-sys` dependencies, and MongoDB 7.0.45. The full tested version/MSRV matrix and evidence boundaries live in [DATABASE_SUPPORT_MATRIX.md](DATABASE_SUPPORT_MATRIX.md). These exact targets do not imply compatibility with every version or vendor-specific feature. MongoDB is implemented through a separate document adapter and does not use SQLx pools or SQL migrations. SQL syntax, placeholders, migrations, and value support remain engine-specific. Bind user values rather than interpolating request data into SQL.

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

### EntityRepository CRUD and query composition

Define entities with SeaORM's `DeriveEntityModel`/`DeriveRelation` derives and RustFlow's optional `ModelDiagnostics` validator. Then bind a repository to the existing pool or an explicit transaction:

~~~rust
use rustflow_server::{
    sea_orm::{ActiveValue::Set, ColumnTrait, QueryFilter, QueryOrder},
    DatabasePool,
};

let accounts = database.entity::<entities::account::Entity>();

let inserted = accounts
    .insert(entities::account::ActiveModel {
        email: Set("ada@example.test".to_owned()),
        enabled: Set(true),
        alias: Set(None),
        metadata: Set(serde_json::json!({})),
        checksum: Set(vec![]),
        ..Default::default()
    })
    .await?;

let found = accounts.find_by_id(inserted.id).await?;
assert!(found.is_some());
let updated = accounts
    .update(entities::account::ActiveModel {
        id: Set(inserted.id),
        alias: Set(Some("Ada Lovelace".to_owned())),
        ..Default::default()
    })
    .await?;

let page = accounts
    .all(
        accounts
            .select()
            .filter(entities::account::Column::Enabled.eq(true))
            .order_by_asc(entities::account::Column::Id)
            .offset(0)
            .limit(25),
    )
    .await?;

let removed = accounts.delete_by_id(updated.id).await?;
assert_eq!(removed.rows_affected, 1);
~~~

The repository surface also includes `one`, `save`, `delete` by ActiveModel, `insert_many`, `update_many`, and `delete_many`. A new model passed to `save` has its generated primary key unset; an existing model has its full primary key set. `update` and `delete` reject ActiveModels with incomplete primary-key state. `update_many`/`delete_many` are SeaORM builders and require an explicit filter unless intentionally acting on the whole table. They return `rows_affected`; bulk insert returns SeaORM's `InsertResult`, whose metadata is not a portable row-count contract.

The `select()` method returns SeaORM's typed `Select<E>` builder. Use typed columns/expressions and bound values for nested filters, comparisons, NULL/IN/LIKE predicates, aliases/projections, joins, grouping, aggregates, HAVING, subqueries, ordering, and pagination. Do not create SQL text from request-provided identifiers or fragments.

### Dialect capabilities and upsert

DatabasePool::capabilities() exposes the selected adapter contract:

| Capability | SQLite | PostgreSQL | MySQL | MariaDB |
|---|---:|---:|---:|---:|
| SeaORM OnConflict upsert builder | Yes | Yes | Yes | Yes |
| INSERT RETURNING in this SeaORM configuration | Yes | Yes | No | No |

The MariaDB row is intentionally conservative: SQLx/SeaORM use one MySQL backend family, and the SeaORM mariadb-use-returning feature remains disabled so it cannot accidentally enable the same behavior for MySQL.

EntityRepository::insert_builder exposes SeaORM's native Insert builder for operations whose semantics differ by dialect:

~~~rust
use rustflow_server::sea_orm::sea_query::OnConflict;

let accounts = database.entity::<entities::account::Entity>();
let result = accounts
    .insert_builder(new_account)
    .on_conflict(
        OnConflict::column(entities::account::Column::Email)
            .update_columns([entities::account::Column::Enabled])
            .to_owned(),
    )
    .exec(database.orm())
    .await?;
~~~

The four-engine ORM contract executes this upsert twice against the same unique key and verifies that the second call updates the row rather than creating another one.

SQLite/PostgreSQL use ON CONFLICT syntax. The MySQL-family builder maps to ON DUPLICATE KEY UPDATE, whose conflict selection is not equivalent to a PostgreSQL/SQLite conflict target: a duplicate on any applicable unique/primary key can trigger the update. Do not assume identical target semantics or use this as a substitute for a deliberate uniqueness design. Check capabilities.insert_returning before calling SeaORM's exec_with_returning; when false, use exec and perform an explicit follow-up read using a stable key.

## Numeric codecs and decimal precision

SeaORM 1.1.20 with SeaQuery uses an SQLite-specific f64 path for both Decimal and BigDecimal: values are bound as floating point and read back through f64. The SQLite fixture therefore uses REAL columns and tests 12.5 only, because it is exactly representable in binary f64. Other decimal-looking values may be rounded or expanded when reconstructed from f64. This is not arbitrary-precision decimal storage; do not use SQLite Decimal/BigDecimal for money or other precision-sensitive values. Use scaled integers or a separately designed canonical-text codec when exact decimal semantics are required on SQLite. PostgreSQL uses NUMERIC, while MySQL and MariaDB use DECIMAL; their integration contracts exercise a high-precision BigDecimal round-trip.

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

DatabaseError::kind() classifies failures without exposing driver messages, SQL, constraint names, values or connection URLs. The same mapping applies to SQLx and SeaORM-originated operations.

| DatabaseErrorKind | Condition | HTTP | Application response |
|---|---|---:|---|
| InvalidConfiguration | Invalid URL, pool settings or migration configuration | 500 | Correct deployment/configuration; never expose the configured URL |
| InvalidModelState | Missing primary-key state or invalid ActiveModel write state | 400 | Correct the model before retrying |
| NotFound | A required row was not found | 404 | Handle missing resource |
| UniqueViolation | Unique or primary-key conflict | 409 | Resolve the conflicting value or use a deliberate upsert path |
| ForeignKeyViolation | Referenced parent missing or referenced row cannot be deleted | 409 | Resolve the relation conflict |
| CheckViolation | A database CHECK constraint rejected the write | 422 | Correct the submitted value |
| Unavailable | Connection/acquire timeout or closed pool | 503 | Retry with backoff where the operation is safe |
| CommitOutcomeUnknown | Transport/protocol failure while COMMIT was in flight | 503 | Inspect the persisted state or idempotency key before retrying |
| Query / Transaction / Migration | Other query, transaction or migration failure | 500 | Handle as an internal database error |

MySQL and MariaDB report a general SQLSTATE for some constraints, so the mapper inspects MySQL protocol error numbers (unique: 1022/1062/1169/1586/1859; foreign key: 1216/1217/1451/1452/1557/1761/1762/1830/1834; CHECK: MySQL 3819, MariaDB 4025 with SQLSTATE 23000). PostgreSQL uses SQLSTATE 23505/23503/23514; SQLite uses 1555/2067/787/275. These mappings are backed by integration tests and are not inferred from an error's text.

A failed COMMIT is not automatically equivalent to a rollback. If the connection is lost before the client receives the server's result, the transaction outcome is uncertain. DatabaseTransaction returns CommitOutcomeUnknown for that case. Do not blindly repeat non-idempotent writes; query a stable application identifier or idempotency record first.

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

Phase 6.6 is complete: its SQL and MongoDB support contracts, runtime versions and evidence are recorded in docs/DATABASE_SUPPORT_MATRIX.md, ADR 0019 and ROADMAP.md. Phase 6.7.1 is complete and merged in PR #58 (36c7d43652d41b3bf92e60cc3ad02405bbfdb1a1); ADR 0020 selects Data Mapper with SeaORM 1.1.20 and the validation-only ModelDiagnostics derive.

Phase 6.7.2 now contains the typed-pool bridge, SeaORM repository facade, CRUD/bulk methods, typed SeaORM query composition, explicit batch loaders, cross-engine codec contracts, shared ORM/raw transactions, typed constraint errors and an explicit ambiguous-COMMIT category. The current PR adds the resolved Cargo.lock and a RustSec audit workflow. This implementation remains under acceptance until all checks pass on the exact PR head and the roadmap criteria are closed with evidence. MongoDB remains a separate document adapter; its ODM is Phase 6.7.3.

## Testing

The ORM contract target executes the same entity/query/relation/type contract on SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 and MariaDB 11.4.13. It covers generated keys, CRUD and bulk writes, nested filters, ordering and offset pagination, joins, projections, grouping/aggregates/HAVING, subqueries, one-to-one/one-to-many/many-to-many batch loading, NULL/JSON/bytes/UUID/date/time/timestamp/numeric codecs, FK/unique/CHECK constraints, and mixed ORM/raw writes inside committed and rolled-back transactions. MockDatabase tests assert batch-loader query counts. The support-specific database_engines target independently verifies MySQL and MariaDB service identity and migration behavior.

The Rust CI matrix runs the workspace and database services under Rust 1.85 and stable, and includes Clippy, tests, Rustdoc and an explicit RustSec cargo audit. Dependency Review remains a separate workflow. The only audit exception is RUSTSEC-2023-0071 for the inherited rsa 0.9.10 dependency of sqlx-mysql 0.8.6; it has no patched release and is documented in ADR 0020 and .cargo/audit.toml. A green audit does not mean that exception is fixed.

Run:

~~~
cargo test -p rustflow-server --test orm_contract -- --test-threads=1
cargo test -p rustflow-server --test database_engines -- --test-threads=1
cargo test -p rustflow-server --all-features
cargo test --workspace --all-features
cargo audit --deny warnings
~~~

The API remains experimental and pre-1.0. Dialect-specific upsert/RETURNING capabilities, generated CRUD application proof, richer relation mutation/nested loading semantics and the separate MongoDB ODM are tracked in ROADMAP.md and later subphases. The session contract remains separate; durable multi-instance session storage needs its own adapter and a security/consistency review.
