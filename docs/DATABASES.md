# SQL ORM and MongoDB ODM, pools, transactions, and migrations (Phase 6.7)

The database API remains experimental and pre-1.0. Phase 6.7.1 selected SeaORM 1.1.20 and defined the Data Mapper/model contract. Phase 6.7.2 was merged through PR #59 on 2026-10-31 (merge commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`): SeaORM runs in `rustflow-server` over one backend-specific SQLx pool shared by SQLx and SeaORM. `EntityRepository` exposes CRUD/bulk operations, typed SeaORM query composition, explicit relation loaders, parameterized raw SQL and mixed ORM/SQL transactions. SQLite, PostgreSQL, MySQL and MariaDB integration contracts, expanded boundary/relationship/COMMIT-fault tests, and all eight required checks passed on the final PR head. MongoDB remains a separate document adapter with its own typed ODM, introduced in Phase 6.7.3. Phase 6.7.4 was accepted through PR #62 and merged into `main` as `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`: the advanced API adds per-relation filters/order, has-many DTO projection, named bounded-depth relation graphs and explicit attach/detach/one-to-one replacement helpers. It preserves explicit batch loading and the one-pool Data Mapper boundary; it does not add lazy loading, per-parent top-N, implicit cascades or helper-managed commits.

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

### Filtered and ordered relationship loading

The RustFlow loaders accept an entity or a typed SeaORM Select through EntityOrSelect. A relation-specific query can therefore filter and order the related rows while keeping the batch loader responsible for the relationship grouping:

~~~rust
use rustflow_server::orm::{load_many, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, QueryFilter, QueryOrder};

let posts_by_user = load_many(
    &users,
    RelationLoadOptions::<entities::post::Entity>::new().modify(|query| {
        query
            .filter(entities::post::Column::Published.eq(true))
            .order_by_desc(entities::post::Column::CreatedAt)
            .order_by_asc(entities::post::Column::Id)
    }),
    database.orm(),
)
.await?;
~~~

The filter applies only to the related query; it does not filter the parent user query. Multiple order clauses are retained. Add a stable tiebreaker such as the related primary key when timestamp or price values can tie. A limit on this Select applies to the complete related batch, not independently per parent; per-parent top-N loading is not part of this API. Values built through SeaORM expressions remain bound parameters.

### Relationship projections

A partial column selection cannot safely be decoded into a complete SeaORM Model. Use a projection DTO with FromQueryResult and load_many_projected. The DTO must expose the related foreign key so the result can be grouped back to its parent:

~~~rust
use rustflow_server::orm::{load_many_projected, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, FromQueryResult, QueryOrder};

#[derive(Clone, Debug, FromQueryResult)]
struct PostSummary {
    id: i64,
    user_id: i64,
    title: String,
}

let summaries = load_many_projected(
    &users,
    entities::post::Column::UserId,
    RelationLoadOptions::<entities::post::Entity>::new().modify(|query| {
        query
            .select_only()
            .column(entities::post::Column::Id)
            .column(entities::post::Column::UserId)
            .column(entities::post::Column::Title)
            .order_by_desc(entities::post::Column::Id)
    }),
    |user| user.id,
    |post: &PostSummary| post.user_id,
    database.orm(),
)
.await?;
~~~

The helper adds a bound IN predicate for all parent keys, executes one query, and groups the decoded DTOs in the returned parent order. The parent-key and DTO-key extractors must identify the same association key and return the same Rust key type. The DTO must include the foreign key selected by the query; otherwise rows cannot be grouped correctly. D must be Clone so duplicate parent keys receive their own grouped values. This helper currently covers has-many projections and returns DTOs, not entity models.

### Configurable nested relation graphs

Use RelationGraph to describe multiple explicit branches and nested batch hops. Each node may use its own RelationLoadOptions, so one branch can be filtered or ordered independently of its siblings:

~~~rust
use rustflow_server::orm::{RelationGraph, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, QueryOrder};

let post_graph = RelationGraph::<entities::post::Model, _>::new()
    .with_many(
        "comments",
        RelationLoadOptions::<entities::comment::Entity>::new()
            .modify(|query| query.order_by_asc(entities::comment::Column::CreatedAt)),
        RelationGraph::new(),
    )?;

let graph = RelationGraph::<entities::user::Model, _>::new()
    .with_many(
        "posts",
        RelationLoadOptions::<entities::post::Entity>::new()
            .modify(|query| query.order_by_desc(entities::post::Column::CreatedAt)),
        post_graph,
    )?
    .with_one(
        "profile",
        RelationLoadOptions::<entities::profile::Entity>::new(),
        RelationGraph::new(),
    )?
    .with_many_to_many(
        "roles",
        RelationLoadOptions::<entities::role::Entity>::new(),
        entities::user_role::Entity,
        RelationGraph::new(),
    )?
    .with_max_depth(4)?;

let loaded = graph.load(&users, database.orm()).await?;
let posts_by_user = loaded.many::<entities::post::Model>("posts").unwrap();
let comments_by_post = loaded.many::<entities::comment::Model>("posts.comments").unwrap();
let profiles = loaded.one::<entities::profile::Model>("profile").unwrap();
let roles_by_user = loaded.many::<entities::role::Model>("roles").unwrap();
~~~

The default maximum graph depth is 8; with_max_depth accepts 1 through 16. Sibling names must be unique path segments. Read results with many for collection nodes and one for one-to-one nodes. Nested-path groups correspond to the flattened models from the preceding collection edge. A many-to-many edge uses the explicit junction entity and performs junction/target batch work; query count depends on edge kind, not parent count. Empty intermediate collections do not trigger downstream queries. There is no implicit lazy loading.

Projected DTO branches can also be added to a graph with with_many_projected. They expose grouped DTO values through many, but do not accept nested entity branches because a partial DTO is not necessarily an Entity Model.

### Typed relation mutation helpers

The relation mutation helpers update an existing related model's typed foreign-key column. They never create or delete related rows and never request a cascade. For a one-to-many association, attach_one_to_many can also reparent an existing child:

~~~rust
use rustflow_server::orm::attach_one_to_many;

let transaction = database.begin_transaction().await?;
let reassigned_post = attach_one_to_many(
    existing_post,
    entities::post::Column::UserId,
    user.id.into(),
    transaction.connection(),
)
.await?;
transaction.commit().await?;
~~~

detach_one_to_many sets the supplied nullable foreign key to NULL. A non-nullable FK, uniqueness constraint, or foreign-key violation is returned as a database error; no fallback delete occurs.

replace_one_to_one detaches the previous related model (if any) and associates an existing replacement inside a caller-owned DatabaseTransaction:

~~~rust
use rustflow_server::orm::replace_one_to_one;

let transaction = database.begin_transaction().await?;
let replacement = replace_one_to_one(
    Some(previous_profile),
    replacement_profile,
    entities::profile::Column::UserId,
    user.id.into(),
    &transaction,
)
.await?;
transaction.commit().await?;
~~~

For true one-to-one semantics, the FK must be nullable and protected by a UNIQUE constraint. The helper does not commit or roll back on behalf of the caller: roll back when it returns an error, and commit only after the complete mutation succeeds. Use EntityRepository::delete / delete_by_id when deletion is intended; database FK actions remain schema-defined.

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

MongoDB is document-oriented and uses its own adapter, independent of SQLx, typed SQLx pools, SQL transactions, and SQL migrations. Phase 6.6.2 provides the low-level official-driver adapter and native collection escape hatch. Phase 6.7.3 adds an experimental typed ODM facade over the same `MongoDatabase` client and official driver; it does not introduce another client/pool or reuse SeaORM entities, SQL relationships, or migrations.

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

The bootstrap contract creates a missing collection, creates declared indexes (including explicitly unique indexes), and verifies each declared index's name, ordered key document, and uniqueness. Re-running an unchanged declaration is intended to be idempotent. It does not drop resources, replace indexes, validate arbitrary document schemas, or implement versioned document migrations. The current `DocumentIndex` surface manages only keys and uniqueness; TTL, sparse, partial, collation and other index options are not claimed. A unique-index violation is mapped to HTTP 409 only when the MongoDB driver reports duplicate-key code 11000, including native write errors; other internal driver errors are not exposed in client-facing responses.

### Typed MongoDB ODM (Phase 6.7.3 — merged into main)

**Acceptance and merge status (2026-10-31):** the ODM implementation and its compile-fail, unit, replica-set and standalone integration coverage passed CI on final PR #60 head `659f1750d611e57315263216167d43375da23574`. Rust 1.85/stable, Clippy, workspace/database tests, MongoDB 7.0.45 topology tests, Rustdoc, CLI/benchmark smoke, WASM, Browser E2E, documentation site, governance and dependency review all passed. PR #60 was squash-merged into `main` as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. The API remains experimental; this merge does not close Phase 6.7 globally.

The MongoDB ODM is a document Data Mapper, not an extension of the SQL ORM. Models are ordinary Serde values and own no client, collection, session, identity map, or implicit I/O. Each model declares its collection and identifier type through `DocumentModel`; its identifier property must serialize as BSON `_id`. Before writes, RustFlow verifies that the typed ID matches the serialized `_id`. If an insert has no ID, the associated `Id` type must be able to deserialize a BSON `ObjectId`; otherwise the ODM rejects the operation before sending it. Custom String/UUID IDs must be supplied explicitly. `replace_one` requires an explicit model ID.

```rust
use rustflow_server::{
    DocumentField, DocumentFilter, DocumentModel, DocumentQuery, DocumentUpdate,
    MongoDatabase,
    mongodb_driver::bson::oid::ObjectId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    email: String,
    age: i32,
}

impl DocumentModel for User {
    type Id = ObjectId;
    const COLLECTION: &'static str = "users";

    fn id(&self) -> Option<Self::Id> { self.id.clone() }
    fn set_id(&mut self, id: Self::Id) { self.id = Some(id); }
}

const EMAIL: DocumentField<String> = DocumentField::new("email");
const AGE: DocumentField<i32> = DocumentField::new("age");

async fn users(database: &MongoDatabase) -> Result<(), rustflow_server::MongoError> {
    let users = database.repository::<User>();
    let saved = users.insert(User {
        id: None,
        email: "ada@example.test".to_owned(),
        age: 37,
    }).await?;

    let filter = DocumentFilter::new()
        .eq_field(EMAIL, "ada@example.test".to_owned())?
        .gte_field(AGE, 18)?;
    let page = DocumentQuery::new(filter)
        .sort_desc("age")?
        .limit(50)?;
    let _matches = users.find_many(page).await?;

    users.update_one(
        DocumentFilter::new().eq_field(EMAIL, "ada@example.test".to_owned())?,
        DocumentUpdate::new().inc_field(AGE, 1)?,
        false,
    ).await?;

    users.find_by_id(saved.id.expect("insert populated the ObjectId")).await?;
    Ok(())
}
```

The repository exposes `insert`, `insert_many`, `find_by_id`, `find_one`, `find_many`, `find_many_as`, `count`, `exists`, `replace_one`, `update_one`, `update_many`, `delete_one`, `delete_many`, and `aggregate_as`. `insert_many` accepts 1–1,000 documents. Ordered bulk insert stops on the first write error; unordered bulk insert may continue. A batch can partially apply before an error, and neither mode is a transaction. The ODM preserves the driver's write error and does not add application-level retries; eligible single-document operations may still use native retryable-write behavior.

`DocumentFilter` composes equality/comparison, boolean AND/OR, existence, null, membership, array predicates and nested dotted paths. `DocumentQuery` supports sorting, skip/limit pagination and inclusion/exclusion projections. `DocumentUpdate` supports `$set`, `$unset`, `$inc`, `$push` and `$pull`. Field paths reject empty segments, NUL/control characters, and `$`-prefixed segments. Update helpers also reject writes to `_id` and nested `_id.*` paths because MongoDB document identifiers are immutable. `DocumentField<T>` statically associates a declared field path with the Rust value type accepted by typed filter/update helpers; the application still owns the mapping between that path and its Serde field name. Use `DocumentFilter::native` only for trusted, deliberately constructed BSON operators outside the helper surface; it does not validate query authorization scope or semantics.

For projections, use `find_many_as::<Dto>` and define a DTO whose required fields match the selected projection. Do not deserialize a partial projection into the full model unless all required model fields are included. For aggregation, pass native MongoDB pipeline stages to `aggregate_as::<ResultDto>`; the ODM does not translate SQL joins or SQL aggregation syntax. Embedded structs and arrays use ordinary BSON nested documents. References are explicit IDs and require explicit repository reads or aggregation stages such as `$lookup`; there is no lazy loading or automatic cascade behavior.

Serde controls missing/null/default behavior. With ordinary `Option<T>`, an absent field and explicit BSON `null` both normally deserialize to `None`; use a custom tri-state representation or raw BSON if that distinction matters. `#[serde(default)]` supplies a value when a field is absent; it does not preserve missing-versus-null information.

The ODM retains the driver's native collection API as an escape hatch. It does not provide a procedural model derive, SQL-style relationships, automatic cascades, a global identity map, document migrations, or every native bulk-write/index/aggregation option. The index declaration currently manages ordered keys and uniqueness only. The API remains experimental and pre-1.0.

### Sessions and transactions

Sessions are created from the same official client by `MongoDatabase::start_session`. Pass `&mut ClientSession` explicitly to each repository operation participating in a transaction. The current session-aware repository subset is insert, find-by-ID, update-one and delete-one. A session alone is not a transaction. Multi-document transactions require a replica set or sharded deployment; a standalone server is insufficient. The integration CI uses a MongoDB 7.0.45 single-node replica set for commit/abort coverage and a separate standalone server to verify the topology restriction. Parallel operations on one transaction session are unsupported.

The driver reports `UnknownTransactionCommitResult` when the outcome of commit is ambiguous; inspect `MongoError::is_unknown_transaction_commit_result()` and reconcile persisted state rather than assuming rollback or rerunning a non-idempotent operation. Owned repository actions continue running if the caller's configured timeout elapses, avoiding cancellation of an in-flight driver future; a timeout still does not reveal whether the server applied a write. Session-borrowing methods are not wrapped in the generic cancellable timeout path.

## Roadmap status

Phase 6.6 is complete: its SQL and MongoDB support contracts, runtime versions and evidence are recorded in docs/DATABASE_SUPPORT_MATRIX.md, ADR 0019 and ROADMAP.md. Phase 6.7.1 is complete and merged in PR #58 (36c7d43652d41b3bf92e60cc3ad02405bbfdb1a1); ADR 0020 selects Data Mapper with SeaORM 1.1.20 and the validation-only ModelDiagnostics derive.

Phase 6.7.2 was merged through PR #59 on 2026-10-31 at `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. The typed-pool bridge, SeaORM repository facade, CRUD/bulk methods, typed query composition, explicit batch loaders, cross-engine codec contracts, shared ORM/raw transactions, typed constraint errors and ambiguous-COMMIT category are covered by the four-engine integration contract. The expanded integer-width/boundary tests, nested-loader query-count assertions, relation attach/update/detach/cascade tests and PostgreSQL live backend-termination test during COMMIT passed on the final PR head; all eight required checks passed. Phase 6.7.3 was accepted on final PR #60 head `659f1750d611e57315263216167d43375da23574` and merged into `main` as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. The global Phase 6.7 maturity gate remains open.

Phase 6.7.4 was accepted on PR #62 final head `28218456db8ac323731054e291023a0765474125` and squash-merged into `main` on 2026-10-31 as `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`. The PR-head CI, Documentation site, Governance checks and Dependency review workflows all passed. The accepted surface includes filtered/ordered `RelationLoadOptions`, `load_many_projected` for has-many DTOs, explicit `RelationGraph` traversal (default depth 8, maximum 16), and `attach_one_to_many`, `detach_one_to_many`, and transaction-bound `replace_one_to_one`. Limit/offset remains batch-wide; projected graph nodes are leaves; attach/detach never delete; callers own transaction boundaries. The generated CRUD application is now accepted in Phase 6.7.5; the global Phase 6.7 maturity gate remains open only for the final quality/compatibility work in Phase 6.7.6.

## Testing

The ORM contract target executes the same entity/query/relation/type contract on SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 and MariaDB 11.4.13. It covers generated keys, CRUD and bulk writes, nested filters, ordering and offset pagination, joins, projections, grouping/aggregates/HAVING, subqueries, one-to-one/one-to-many/many-to-many batch loading, NULL/JSON/bytes/UUID/date/time/timestamp/numeric codecs, FK/unique/CHECK constraints, and mixed ORM/raw writes inside committed and rolled-back transactions. The expanded branch contract round-trips signed `i8/i16/i32/i64` boundaries on all four engines, tests supported unsigned widths and overflow behavior on SQLite/MySQL/MariaDB, asserts one query per nested loader hop, exercises junction attach/update/detach/cascade, and injects a PostgreSQL backend termination while COMMIT is in flight. The expanded Phase 6.7.2 cases passed on the final PR head before PR #59 was merged. The independent Phase 6.7.3 MongoDB integration suite passed on final PR #60 head `659f1750d611e57315263216167d43375da23574`, including replica-set commit/abort, standalone transaction rejection, nested-document/projection, CRUD/bulk/query/index/ID tests and all required CI checks. PR #60 was merged into `main` as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. The support-specific `database_engines` target independently verifies MySQL and MariaDB service identity and migration behavior.

The Phase 6.7.4 ORM integration additions execute on SQLite, PostgreSQL, MySQL and MariaDB and cover filtered/ordered relation loads, DTO projection grouping, multi-branch and nested relation graphs, absent related rows, reparenting, one-to-one replacement, detach and rollback. Mock-backed tests assert one query for the projected loader and one batched query per nested graph hop; query counts do not grow per parent.

The Rust CI matrix runs the workspace and database services under Rust 1.85 and stable, and includes Clippy, tests, Rustdoc and an explicit RustSec cargo audit. Dependency Review remains a separate workflow. The only audit exception is RUSTSEC-2023-0071 for the inherited rsa 0.9.10 dependency of sqlx-mysql 0.8.6; it has no patched release and is documented in ADR 0020 and .cargo/audit.toml. A green audit does not mean that exception is fixed.

Run:

~~~
cargo test -p rustflow-server --test orm_contract -- --test-threads=1
cargo test -p rustflow-server --test database_engines -- --test-threads=1
cargo test -p rustflow-server --all-features
cargo test --workspace --all-features
cargo audit --deny warnings
~~~

The API remains experimental and pre-1.0. PostgreSQL unsigned primitive mapping is unavailable in the selected SeaORM 1.1.20 foundation; unsigned `u8/u16/u32` coverage is therefore limited to SQLite and MySQL/MariaDB, while `u64` is MySQL/MariaDB-only. Dialect-specific upsert/RETURNING behavior and the ODM's explicit session-operation subset remain documented limitations. Phase 6.7.5's generated CRUD integration is accepted and merged; the remaining global maturity gate is Phase 6.7.6. The independent MongoDB ODM is merged in `main` through PR #60 and remains experimental/pre-1.0. The session contract remains separate; durable multi-instance session storage needs its own adapter and a security/consistency review.
## Phase 6.7.6 quality, compatibility and benchmark validation

The final quality gate adds repeatable tests for behavior that a green CRUD path does not establish:

- The SQL ORM integration contract runs pool-exhaustion tests on SQLite, PostgreSQL, MySQL and MariaDB. Each test holds the only configured pool connection, verifies that the next operation returns within the acquire-timeout bound as `DatabaseErrorKind::Unavailable`, releases the transaction, and confirms the pool works again.
- Concurrent ORM writes run as 32 independent tasks against each supported SQL engine and verify that all distinct rows are visible after completion.
- SQL ORM and MongoDB ODM query-safety regressions pass SQL/operator-looking strings as ordinary values and verify exact matching rather than interpreting the string as SQL syntax or BSON operators.
- `ModelDiagnostics` already has compile-fail rustdoc examples for invalid entity attributes, nullable primary keys and unsupported field types; the workspace doctest lane executes these examples alongside the unit tests.

The reproducible performance smoke baseline is `cargo bench -p rustflow-server --bench persistence`. It reports elapsed time and nanoseconds per operation for ORM insert/find/update/delete, bulk insert, typed query composition/execution, batched relationship loading (20 parents × 4 children), and MongoDB ODM CRUD/bulk operations. SQL targets run for each configured `RUSTFLOW_TEST_DATABASE_URL`, `RUSTFLOW_TEST_MYSQL_URL` and `RUSTFLOW_TEST_MARIADB_URL`, plus bundled SQLite; the MongoDB measurements run when `RUSTFLOW_TEST_MONGODB_URI` is present. CI supplies the pinned service images and runs this baseline on the stable toolchain. Locally, set `RUSTFLOW_BENCH_ITERATIONS` and `RUSTFLOW_BENCH_BULK_SIZE` to control repetitions and batch size.

These numbers are comparative smoke baselines, not a release performance claim or a statistically rigorous benchmark suite. Record the commit, `rustc --version`, Cargo.lock, database image/version, runner CPU and configured pool size when comparing results. Do not compare results collected on different service versions or runner classes as though they were controlled experiments.
