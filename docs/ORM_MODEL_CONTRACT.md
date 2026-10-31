# ORM model and SQL type contract (Phase 6.7.1)

**Status: Phases 6.7.1–6.7.4 are merged; the independent MongoDB ODM from 6.7.3 is also merged; global Phase 6.7 remains in progress.** SeaORM 1.1.20 and the Data Mapper contract were selected in 6.7.1. Phase 6.7.2 adds the server-only SeaORM dependency/facade, one backend-specific typed pool shared by SQLx and SeaORM, `EntityRepository`, typed query execution, explicit relation loaders, mixed ORM/raw transactions, engine-aware error classification and runtime codec fixtures. Its four-engine contract and expanded boundary/failure tests passed on final PR #59 head `2cadce73c19c37c2f088f572ea37a83c65c72380`; PR #59 merged into `main` as `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. Phase 6.7.3 adds a separate MongoDB BSON/Serde ODM; PR #60 merged into `main` as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. The type/engine matrix below distinguishes tested SQL cases from explicit limitations and unverified combinations; MongoDB's document contract is covered separately in [DATABASES.md](DATABASES.md) and ADR 0021.

## Selected architecture and model declaration

RustFlow selects a Data Mapper architecture backed by SeaORM 1.1.20. A model is a data value; it does not contain a connection. SeaORM generates its Entity, Model, ActiveModel, Column and PrimaryKey types. The application passes a connection or explicit transaction to operations.

~~~rust
use rustflow::ModelDiagnostics;
use rustflow_server::orm as sea_orm;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
#[sea_orm(table_name = "user_profiles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub display_name: String,
    pub nickname: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
~~~

In a real application, each entity belongs in its own Rust module. <code>ModelDiagnostics</code> is RustFlow validation only. <code>DeriveEntityModel</code> from SeaORM generates the entity/model API and metadata. SeaORM's <code>ActiveModelBehavior</code> provides opt-in lifecycle hooks.

## Entity and field metadata

- A writable entity is a named-field, non-generic Rust struct and must declare an explicit <code>#[sea_orm(table_name = "...")]</code>.
- Table, schema and column identifiers must match the simple ASCII identifier grammar <code>[A-Za-z_][A-Za-z0-9_]*</code>. They are identifiers, not SQL fragments. The future query renderer quotes them for each dialect.
- SeaORM converts Rust field names to SQL column names using its rename rules. <code>column_name</code> overrides an individual name; <code>rename_all</code> controls the entity's field naming convention.
- One or more fields may be primary keys. The supported composite-key arity is at most 12 fields, the limit exposed by the selected SeaORM API. Composite keys must declare <code>auto_increment = false</code>.
- A single auto-increment primary key is supported where the engine and column type provide it. A primary key cannot be nullable.
- <code>Option<T></code> is the only portable nullability wrapper. <code>Option<Option<T>></code>, references, tuple/array fields, <code>isize</code> and <code>usize</code> are rejected. SQL NULL decoded into a non-nullable field is an error, not a default value.
- SeaORM's supported metadata includes <code>nullable</code>, <code>default_value</code>, <code>default_expr</code>, <code>auto_increment</code>, <code>indexed</code>, <code>unique</code>, <code>ignore</code>, <code>column_type</code>, <code>enum_name</code>, <code>select_as</code> and <code>save_as</code>. RustFlow rejects unknown and duplicate attributes in its supported subset. Conflicting default declarations and invalid identifier values are rejected.
- A custom enum requires an explicit SeaORM enum mapping and a type implementing the matching SeaORM <code>ActiveEnum</code>/<code>ValueType</code> contract. A custom column type requires an explicit <code>column_type</code> mapping and a matching <code>ValueType</code> implementation.
- Index/unique/column metadata does not execute DDL. Foreign-key and check constraints remain in engine-specific SQLx migration files. A model derive does not prove that a live table matches the declared entity.
- Keyless read views are not writable SeaORM entities in the first RustFlow contract. Use query-specific DTOs/selection types for projections without a persistent primary key.

## Rust logical-type matrix

The table defines the target contract and expected engine-specific treatment. It is not a declaration that RustFlow's runtime codecs have already passed the required integration tests.

| Rust logical type | SQLite 3.46.0 | PostgreSQL 16.15 | MySQL 8.4.13 LTS | MariaDB 11.4.13 LTS | Required behavior / limitations |
|---|---|---|---|---|---|
| Signed integers i8/i16/i32/i64 | INTEGER storage with per-width range checks | `char` / SMALLINT / INTEGER / BIGINT | TINYINT / SMALLINT / INT / BIGINT | TINYINT / SMALLINT / INT / BIGINT | i8 uses PostgreSQL's one-byte `char` mapping. Runtime contract round-trips each signed minimum/maximum on all four engines and rejects out-of-range inserts for the dialect-supported widths; SQLite enforces narrow widths with CHECK constraints. |
| Unsigned integers u8/u16/u32 | INTEGER storage with per-width range checks | Not available in SeaORM 1.1.20 | TINYINT UNSIGNED / SMALLINT UNSIGNED / INT UNSIGNED | TINYINT UNSIGNED / SMALLINT UNSIGNED / INT UNSIGNED | Runtime round-trip and negative/upper overflow rejection are exercised on SQLite and MySQL/MariaDB. PostgreSQL unsigned primitive mapping is explicitly unsupported by the selected SeaORM foundation; model as a checked signed type or use a custom codec instead. |
| u64 | Not available as a direct SQLx/SeaORM codec | Not available in SeaORM 1.1.20 | BIGINT UNSIGNED; full range | BIGINT UNSIGNED; full range | Not a portable shared-model field. Use i64 for values in the signed range; use u64 only in MySQL-family-specific models. The contract round-trips u64::MAX and rejects negative/greater-than-u64::MAX inserts on MySQL and MariaDB. |
| Floating point f32/f64 | REAL | REAL / DOUBLE PRECISION | FLOAT / DOUBLE | FLOAT / DOUBLE | Approximate values; never substitute for exact decimal arithmetic. |
| Boolean | INTEGER 0/1 representation | BOOLEAN | BOOLEAN/TINYINT representation | BOOLEAN alias/TINYINT representation | Decode only documented backend representations; arbitrary nonzero is not automatically true. |
| String / text | TEXT | TEXT / VARCHAR | TEXT / VARCHAR | TEXT / VARCHAR | Character set and collation depend on schema/server configuration. |
| Bytes Vec<u8> | BLOB | BYTEA | BLOB family | BLOB family | Binary data is not a string; enforce engine/schema size limits. PostgreSQL arrays are a separate opt-in. |
| Decimal / BigDecimal | REAL through SeaORM's f64 codec | NUMERIC/DECIMAL | DECIMAL exact precision/scale | DECIMAL exact precision/scale | SQLite's SeaORM 1.1.20/SeaQuery codec binds and decodes through f64; arbitrary precision and exact decimal arithmetic are not guaranteed there. Use only decimal values exactly representable through the f64 path (the integration fixture uses 12.5), or use scaled integers/a custom string codec. PostgreSQL/MySQL/MariaDB retain native exact numeric storage and are tested with a high-precision BigDecimal. |
| UUID | Explicit canonical text/binary representation required | Native UUID | Explicit canonical text/binary representation required | Explicit canonical text/binary representation required | Do not silently reinterpret byte order or text format. Runtime mapping must round-trip exact values. |
| Date | Explicit text/integer representation | DATE | DATE | DATE | Strict calendar validation and documented encoding. |
| Time | Explicit text/integer representation | TIME | TIME | TIME | Fractional precision is bounded by the server column type; reject or document loss explicitly. |
| Timestamp without timezone | Explicit text/integer representation | TIMESTAMP WITHOUT TIME ZONE | DATETIME | DATETIME | Wall-clock time, not an instant. Never silently attach a local timezone. |
| Timestamp with timezone / instant | Normalize to UTC using documented representation | TIMESTAMPTZ | DATETIME/TIMESTAMP under explicit UTC/session convention | DATETIME/TIMESTAMP under explicit UTC/session convention | MySQL-family types do not preserve arbitrary source timezone. Normalize and test session behavior. |
| Enum | Stable text labels | Stable text labels by default; native PG enum is explicit | Stable text labels or explicit native enum capability | Stable text labels or explicit native enum capability | Persist explicit labels, not Rust discriminants or Debug output. Unknown labels fail decode. |
| JSON | Text representation; validity/query functions depend on SQLite build/schema | JSON/JSONB | JSON | JSON alias/text semantics differ by product | JSON number representation and operators are not assumed portable. |
| Nullable Option<T> | SQL NULL | SQL NULL | SQL NULL | SQL NULL | None maps to NULL only for nullable columns; NULL for T is a decode error. |
| Arrays/ranges/spatial/vendor-specific | Excluded unless explicitly enabled/tested | Excluded unless explicitly enabled/tested | Excluded unless explicitly enabled/tested | Excluded unless explicitly enabled/tested | No automatic portability inference; use a documented per-engine extension. |

## ActiveModel state and operation semantics

SeaORM's ActiveModel has three field states:

- <code>NotSet</code>: the field is omitted from the write payload; the database may apply a default or generated value.
- <code>Set(value)</code>: the field is explicitly written.
- <code>Unchanged(value)</code>: the value was copied from a Model and has not been marked as modified. An insert may include it; an update omits it.

This is per-field write state, not a process-global identity map or session-wide unit of work. Model operations consume/return values rather than secretly mutating the caller's original Model.

- **Insert:** receives explicit insert values. NotSet fields are omitted. Success returns the resulting Model with fields actually returned or subsequently read from the database. RustFlow must not invent generated timestamps, UUIDs or defaults that were never observed.
- **Update:** uses the complete primary key to target one entity, or an explicitly built predicate for bulk updates. Only Set non-key fields are written by an ActiveModel update; NotSet and Unchanged fields are omitted. Missing key values must be checked by RustFlow's safe wrapper and returned as a typed error rather than reaching a lower-level builder panic.
- **Delete:** targets the complete primary key or an explicitly built safe predicate and returns affected-row information. Zero affected rows is distinct from connection/query failure.
- **Read one/many:** validates type conversion and nullability. Partial projections should use explicit selected types, not silently construct a full Model with missing required columns.
- **Generated values/defaults:** single auto-increment key behavior is engine-specific. PostgreSQL and SQLite support RETURNING; MySQL-family behavior is not assumed equivalent. SeaORM's <code>mariadb-use-returning</code> feature is disabled for both products because the shared MySQL backend cannot safely enable MariaDB-only behavior without also enabling it for MySQL. Any follow-up read is explicit and tested.
- **Bulk operations:** supported methods and partial-failure behavior must be documented per engine; a bulk write is not assumed atomic outside an explicit transaction.

## Hooks, transactions and partial failures

SeaORM ActiveModelBehavior provides hooks with the following order:

1. <code>before_save</code> before insert/update.
2. <code>after_save</code> only after a write succeeded and a resulting Model was obtained.
3. <code>before_delete</code> before delete.
4. <code>after_delete</code> after delete succeeded.

Hooks receive the same explicit connection/transaction argument and may return an error. An <code>after_save</code>/<code>after_delete</code> error can happen after the SQL statement succeeded. Without a caller-supplied transaction, the database change may already be committed; the hook error is not an automatic rollback. If the complete hook-plus-write operation must be atomic, the caller wraps it in an explicit transaction.

Operations that need multi-statement atomicity receive a transaction explicitly. A callback error triggers rollback; successful callback completion attempts commit. Failed commit is returned as an error. Connection loss during commit can leave the server outcome unknown; callers need reconciliation/idempotency rather than a false rollback guarantee. MySQL/MariaDB DDL may implicitly commit and is not made atomic by a transaction wrapper.

## Errors and security

Runtime errors must distinguish invalid metadata, invalid input/state, encode/decode, not found, unique/foreign-key/check constraints, connection/pool, query, concurrency and transaction outcomes. Public messages and Debug representations redact connection URLs/passwords and do not reveal raw SQL or bound secrets.

All caller-supplied values are bound parameters. Raw SQL is an explicit escape hatch built with a Statement and bound values or a typed SQLx pool/connection from the one shared pool. SQL identifiers and fragments must be generated from validated metadata/typed expressions. Unprepared SQL is limited to trusted static migration/administrative SQL and never request data.

## SQL ORM / MongoDB ODM boundary

The SQL ORM covers only SQLite, PostgreSQL, MySQL and MariaDB. It uses relational entities, SQL expressions, explicit SQL transactions and engine-specific SQLx migration directories.

MongoDB remains the separate document adapter introduced in Phase 6.6.2. Its 6.7.3 ODM contract owns BSON/document models, _id, serializers/deserializers, collection/index lifecycle, document filters/updates and MongoDB-native session/transaction constraints. SQL model derives, joins, relations and SQL migrations do not imply MongoDB behavior.

## 6.7.1 implementation boundary

Implemented in this subphase: the SeaORM 1.1.20 foundation decision; RustFlow's validation-only ModelDiagnostics derive for the SeaORM attribute subset; positive validator tests; compile-fail rustdoc cases for unsupported entity attributes, nullable keys and unsupported field types; model/operation/type/hook/transaction contract; ADR and public/technical documentation.

Not implemented in this subphase: adding the SeaORM dependency/facade; CRUD execution through RustFlow's current DatabasePool; the typed-pool bridge; runtime row codecs and cross-engine type evidence; RustFlow's integrated query execution API; RustFlow relation mutation/loading wrappers; ODM; automatic DDL or migrations. Those are tracked in the later mandatory subphases.

## References

See [ADR 0020](adr/0020-orm-model-contract.md) for the verified foundation comparison, pool migration, design trade-offs and official upstream links. See [DATABASE_SUPPORT_MATRIX.md](DATABASE_SUPPORT_MATRIX.md) for current RustFlow service versions and existing SQLx/MongoDB evidence.
## 6.7.2 SQL ORM runtime implementation (merged; experimental API)

The Phase 6.7.2 runtime slice below was accepted on PR #59 head `2cadce73c19c37c2f088f572ea37a83c65c72380`; Rust 1.85/stable, four-engine integration, dependency review, documentation, WASM/browser, governance and Cloudflare Pages checks completed successfully (8/8). PR #59 was merged into `main` on 2026-10-31 at `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. The API remains experimental and pre-1.0.

### Connection and repository boundary

- rustflow-server owns SeaORM 1.1.20. It is not pulled into the core, view, DOM, browser or WASM crates.
- DatabasePool owns exactly one typed SQLx pool for the selected backend (SqlitePool, PgPool or MySqlPool). SeaORM is constructed from a clone of that handle; there is no second pool.
- sqlx_sqlite_pool(), sqlx_postgres_pool() and sqlx_mysql_pool() expose only a backend-matching pool. The MySQL accessor also serves MariaDB through the MySQL protocol.
- DatabasePool::entity::<E>() and DatabaseTransaction::entity::<E>() return repositories borrowing the existing connection/transaction. A repository never creates a pool or performs lazy I/O.
- Raw SQL goes through bound Statement values via query_raw / execute_raw, or through the typed SQLx accessor. Caller values are parameters, not SQL interpolation. SQLx continues to own engine-specific migrations.

### Entity operations and query composition

EntityRepository exposes select, all, one, find_by_id, insert, update, save, delete, delete_by_id, insert_many, update_many and delete_many. Update/delete by ActiveModel validate that the primary key is present before delegating to SeaORM. Generated values are returned from the database, and bulk methods return SeaORM's affected-row/result metadata.

The query builder deliberately uses SeaORM's typed query surface rather than introducing a second SQL AST. The four-engine integration contract exercises nested AND/OR filters, equality/comparison operators, `IS NULL`/`IS NOT NULL`, parameterized `IN` and text matching, ordering, distinct, limit/offset pagination, joins, a projected/aliased column, grouping, aggregates, HAVING and an `IN` subquery. Values are bound through SeaORM's expression/query APIs; identifiers and expressions are represented by typed columns/expressions rather than request-string interpolation. Backend SQL generation and driver value codecs remain SeaORM/SQLx behavior; this is not a promise that arbitrary SQL fragments are portable.

### Relations and explicit loading

Relationship metadata is defined on SeaORM entities. RustFlow exports explicit batch helpers `load_one`, `load_many` and `load_many_to_many`. They take parent models, the related entity (and junction entity for many-to-many), and an explicit connection. The four-engine integration contract exercises belongs-to, has-one, has-many and many-to-many relations, including absent has-one/has-many/many-to-many results. Mock query-count tests assert one query for a has-many batch, two for a many-to-many batch, and exactly one query per hop for a two-hop graph; the number of parent models does not add per-parent queries. The nested fixture also verifies that an absent belongs-to target is returned as `None`. There is no transparent lazy loading. The four-engine contract now exercises junction attach, update, detach and foreign-key cascade deletion, in addition to a transactional junction-delete rollback.


Phase 6.7.4 extends this baseline with higher-level explicit APIs, while preserving these semantics. `RelationLoadOptions<E>` wraps SeaORM's `Select<E>` so a batch edge can carry relation-specific filters and ordering. `load_many_projected` decodes a caller-defined DTO instead of pretending a partial select is a complete Model; callers provide parent-key and related-FK extractors so results can be grouped safely. `RelationGraph` composes multiple named branches and nested batch hops with an explicit maximum depth; results are read by path with typed `many` / `one` accessors. The graph has no automatic execution or lazy I/O. The current interface applies limit/offset to the whole related batch, not per-parent top-N. Projected graph branches return DTO collections and cannot themselves have nested entity branches. These additions were accepted and merged through PR #62 on 2026-10-31 (final PR head `28218456db8ac323731054e291023a0765474125`; squash merge `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`). The APIs remain experimental/pre-1.0. `RelationGraph` defaults to depth 8 and supports a configured maximum of 16; edge query count depends on graph shape, not parent count. A `Select` limit/offset applies to the whole related batch, not per parent, and projected has-many graph branches are leaves. See [ADR 0022](adr/0022-advanced-sql-relation-loading.md) for the accepted design and explicit limits.

Typed mutation helpers are explicit as well: `attach_one_to_many` reassigns an existing child, `detach_one_to_many` sets a nullable FK to NULL, and `replace_one_to_one` detaches the previous row and associates an existing replacement inside the caller's `DatabaseTransaction`. The first two accept an explicit `ConnectionTrait` (a connection or transaction); the one-to-one replacement helper requires a caller-owned transaction. They do not create/delete rows, commit/rollback for the caller, or override schema-defined FK actions. A true one-to-one FK must be nullable and protected by a UNIQUE constraint.

### Transactions and error contract

ORM calls and parameterized raw statements can use the same DatabaseTransaction. The four-engine contract checks mixed ORM/raw writes under both rollback and commit. A COMMIT transport/protocol failure is classified as DatabaseErrorKind::CommitOutcomeUnknown; the server cannot claim the transaction was rolled back and clients must inspect application state/idempotency before retrying.

DatabaseError::kind() exposes the stable categories InvalidConfiguration, InvalidModelState, NotFound, UniqueViolation, ForeignKeyViolation, CheckViolation, Unavailable, Query, Transaction, CommitOutcomeUnknown and Migration. Public HTTP mapping is intentionally generic: invalid model state 400, missing row 404, unique/FK conflict 409, check constraint 422, unavailable/unknown commit 503, and unexpected query/transaction/migration/configuration failures 500. Database messages, constraint names, values and URLs are not placed in public responses.

Constraint classification uses SQLSTATE/SQLite codes for PostgreSQL/SQLite and MySQL numeric server codes for MySQL/MariaDB. Server-native constraint semantics are not assumed identical across engines.

### Dialect-specific insert behavior

DatabaseBackend::capabilities() and DatabasePool::capabilities() report two explicit facts: the SeaORM OnConflict builder is available on all four engine targets, while INSERT RETURNING is advertised only for SQLite and PostgreSQL in this build. The MariaDB-specific SeaORM feature stays disabled because the MySQL/MariaDB protocol family is shared with MySQL.

EntityRepository::insert_builder returns SeaORM's native Insert<ActiveModel> builder, so callers can construct an OnConflict policy without concatenating SQL. The 4-engine contract executes an insert followed by an upsert against the same unique email and asserts that the existing row is updated. SQLite/PostgreSQL generate ON CONFLICT; MySQL/MariaDB generate ON DUPLICATE KEY UPDATE. The latter may react to any unique/primary-key duplicate and must not be described as having exactly the same conflict-target semantics.

Check DatabaseCapabilities::insert_returning before requesting exec_with_returning. The MySQL/MariaDB adapter reports false; use exec plus a deliberate follow-up read by a stable key instead. This is a capability statement for the current SeaORM feature configuration, not a statement that MariaDB servers lack all SQL RETURNING support.

### Type and engine evidence

The integration target runs the same ORM contract against bundled SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS and MariaDB 11.4.13 LTS. In addition to generated keys, CRUD/bulk, defaults, constraints and scalar codecs, the expanded integer contract round-trips the minimum and maximum of `i8/i16/i32/i64` on all four engines and rejects out-of-range SQL inserts for the dialect-supported widths. SQLite checks `i8/i16/i32` and `i64` storage bounds; PostgreSQL uses its one-byte `char` mapping for `i8`, with native `SMALLINT/INTEGER/BIGINT` for the remaining signed widths; MySQL/MariaDB use `TINYINT/SMALLINT/INT/BIGINT`. Unsigned `u8/u16/u32` round-trips and lower/upper overflow failures are tested on SQLite and MySQL/MariaDB; PostgreSQL unsigned primitive fields are explicitly unsupported by SeaORM 1.1.20. Full-range `u64` and below-zero/above-maximum rejection are tested only on MySQL/MariaDB.

Known explicit limitations:
- SQLx/SeaORM do not provide a portable direct u64 codec for SQLite/PostgreSQL. Full-range u64 is exercised only with MySQL/MariaDB BIGINT UNSIGNED and engine-specific models.
- SeaORM 1.1.20's SQLite Decimal/BigDecimal codec passes through f64. The SQLite fixture uses the exactly representable value 12.5; arbitrary precision and exact decimal arithmetic are not guaranteed there. Use scaled integers or a canonical custom codec when exact decimal semantics are required. PostgreSQL NUMERIC and MySQL/MariaDB DECIMAL are exercised with a high-precision value.
- MySQL and MariaDB share the SQLx MySQL driver/backend enum. The scheme selects the protocol family; server identity is asserted separately in integration CI.
- Dialect-specific upsert/RETURNING behavior and server capability detection remain explicit engine concerns; the shared facade does not pretend these features have identical semantics.

### Dependency and security gate

Cargo.lock is regenerated from the workspace manifest under Rust 1.85 and includes the pinned SeaORM 1.1.20 dependency graph. CI runs the GitHub dependency review and a locked cargo audit. The only audit exception is RUSTSEC-2023-0071 for rsa 0.9.10 inherited through sqlx-mysql 0.8.6. The advisory still has no patched release. SQLx's MySQL client uses RSA public-key encryption in the non-TLS password exchange path; this source-level reachability note does not mean the advisory is fixed or that TLS is enabled. The exception must be revisited when driver/authentication configuration changes and removed when the dependency is eliminated or fixed.

### Remaining scope after the 6.7.2 runtime slice

The expanded runtime tests for the signed/unsigned widths supported by each dialect, integer boundary failures, nested loader query counts, relation attach/update/detach/cascade semantics, and PostgreSQL backend termination while a deferred trigger is executing inside COMMIT all passed on the exact PR head stated above. Phase 6.7.2 was subsequently merged into `main` through PR #59 on 2026-10-31. The generated CRUD application remains outside this SQL runtime subphase and is tracked by Phase 6.7.5. The advanced relation/loading extensions have since been delivered in Phase 6.7.4 through PR #62 and merged into `main` as `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`. The separate MongoDB ODM was added in Phase 6.7.3 and merged into `main` through PR #60. The global Phase 6.7 maturity gate remains open for the generated application and Phase 6.7.6 quality/compatibility work.
