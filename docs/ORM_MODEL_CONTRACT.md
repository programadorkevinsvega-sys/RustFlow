# ORM model and SQL type contract (Phase 6.7.1)

**Status: Phase 6.7.1 accepted; Phase 6.7.2 implementation in progress.** SeaORM 1.1.20 and the Data Mapper contract were selected in Phase 6.7.1. The Phase 6.7.2 branch adds the server-only SeaORM dependency/facade and replaces SQLx `AnyPool` with backend-specific typed pools shared by SQLx and SeaORM. This establishes the connection boundary, not completion of ORM runtime behavior: CRUD, relation loading, checked type codecs and the full four-engine integration matrix remain unverified. The type/engine matrix below is the target contract, not runtime evidence.

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
| Signed integers i8/i16/i32/i64 | SQLite INTEGER storage | SMALLINT / INTEGER / BIGINT | TINYINT / SMALLINT / INT / BIGINT | TINYINT / SMALLINT / INT / BIGINT | Checked narrowing on decode; reject overflow, never wrap. Exact DDL width remains schema-specific. |
| Unsigned integers u8/u16/u32 | INTEGER with range check | Signed integer representation with checked range | Unsigned integer types where defined | Unsigned integer types where defined | Reject values outside backend-supported ranges. |
| u64 | No direct SQLx/SeaORM value codec | No direct SQLx/SeaORM value codec | BIGINT UNSIGNED; full range | BIGINT UNSIGNED; full range | Not a portable shared-model field. SQLx rejects u64 on SQLite/PostgreSQL even when the value fits i64. Use i64 for portable signed-range values; use u64 only in MySQL-family-specific models. The integration contract tests u64::MAX on both MySQL and MariaDB. |
| Floating point f32/f64 | REAL | REAL / DOUBLE PRECISION | FLOAT / DOUBLE | FLOAT / DOUBLE | Approximate values; never substitute for exact decimal arithmetic. |
| Boolean | INTEGER 0/1 representation | BOOLEAN | BOOLEAN/TINYINT representation | BOOLEAN alias/TINYINT representation | Decode only documented backend representations; arbitrary nonzero is not automatically true. |
| String / text | TEXT | TEXT / VARCHAR | TEXT / VARCHAR | TEXT / VARCHAR | Character set and collation depend on schema/server configuration. |
| Bytes Vec<u8> | BLOB | BYTEA | BLOB family | BLOB family | Binary data is not a string; enforce engine/schema size limits. PostgreSQL arrays are a separate opt-in. |
| Decimal / BigDecimal | REAL through SeaORM's f64 codec | NUMERIC/DECIMAL | DECIMAL exact precision/scale | DECIMAL exact precision/scale | SQLite's SeaORM 1.1.20/SeaQuery codec binds and decodes through f64; arbitrary precision and exact decimal arithmetic are not guaranteed there. Use only values proven to round-trip through f64, or use scaled integers/a custom string codec. PostgreSQL/MySQL/MariaDB retain native exact numeric storage and are tested with a high-precision BigDecimal. |
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
## 6.7.2 implementation boundary (in progress)

The first implementation slice in the Phase 6.7.2 branch provides:

- SeaORM 1.1.20 is added as a dependency of `rustflow-server` only, with SQLite, PostgreSQL and MySQL SQLx drivers and Tokio/Rustls.
- `DatabasePool` owns one driver-specific SQLx pool (`SqlitePool`, `PgPool` or `MySqlPool`). The SeaORM `DatabaseConnection` is constructed from a clone of that exact pool handle; it does not open a second pool.
- The existing configuration URL precedence, redaction, limits, acquire timeout and backend selection remain in RustFlow's `DatabaseConfig`.
- Direct SQLx access is explicit through backend-checked accessors: `sqlx_sqlite_pool()`, `sqlx_postgres_pool()`, and `sqlx_mysql_pool()`.
- `DatabasePool::orm()`, `query_raw`, `execute_raw`, and the SeaORM-backed `DatabaseTransaction` expose the shared connection to ORM and parameterized raw statements. SQLx migrations dispatch to the matching typed pool.
- MongoDB remains separate and is not included in SeaORM or the browser/WASM graph.

Still required before Phase 6.7.2 can be considered complete: model insert/update/delete and bulk behavior; composable filter/join/group/order/pagination/subquery/aggregate coverage; explicit relationship metadata and batched `load_one`/`load_many`/`load_many_to_many` behavior; N+1/query-count tests; runtime codecs and NULL/range/precision tests on SQLite, PostgreSQL, MySQL and MariaDB; mixed ORM/raw-SQL transaction tests with commit/rollback and ambiguous commit outcomes; diagnostics and typed error mapping; dependency-lock and RustSec review (including the SQLx MySQL/RSA advisory already recorded in ADR 0020); runnable technical and public-site examples; and all CI/MSRV/WASM/security gates. Do not mark the roadmap criterion complete based solely on the connection bridge.