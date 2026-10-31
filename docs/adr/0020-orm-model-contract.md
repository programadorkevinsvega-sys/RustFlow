# ADR 0020: SQL ORM architecture, foundation and model contract

- Status: Accepted with Phase 6.7.1 (PR #58, merge commit `36c7d43652d41b3bf92e60cc3ad02405bbfdb1a1`); Phase 6.7.2 accepted and merged in PR #59 (merge commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`); Phase 6.7.4 accepted and merged in PR #62 (merge commit `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`).
- Phase: 6.7.1 — Model contract and architecture.
- Related: ADR 0016 (SQLx pools and migrations), ADR 0017 (MySQL/MariaDB driver), ADR 0018 (MongoDB document adapter), ADR 0019 (database support matrix), [ORM model contract](../ORM_MODEL_CONTRACT.md).

## Context

RustFlow's Phase 6.6 support contract is verified for bundled SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS and MongoDB 7.0.45. The workspace MSRV is Rust 1.85; CI verifies that toolchain and stable. The existing SQL adapter is pinned to SQLx 0.8.6 and exposes an AnyPool, explicit transaction wrappers, parameterized SQL and SQLx migrations. MongoDB uses the official MongoDB Rust driver 3.6.0 and a separate BSON/collection/index adapter.

Before this proposal, RustFlow had no integrated SQL ORM, model-row mapping, query execution API or relation loader. The CLI's generated models were plain structs and did not imply persistence. Phase 6.7 must be a cohesive, mature ORM for SQLite, PostgreSQL, MySQL and MariaDB, plus an independently designed MongoDB ODM. It must not close as a CRUD-only MVP.

The key constraint is not simply whether an ORM has a Rust 1.85-compatible derive. It must also share the existing SQLx generation, support the intended four SQL servers, offer a query builder, relationships and batch loading, explicit transactions, type mapping and diagnostic behavior, while preserving a single managed connection pool per configured database. A generic SQLx Any driver does not prove that each backend has identical type codecs, SQL syntax, generated-value behavior or transaction semantics.

## Decision

### 1. Pattern: Data Mapper with explicit ActiveModel write state

Choose **Data Mapper**, implemented on the maintained SeaORM foundation. Do not build a custom Active Record or a custom ORM.

- A SeaORM Model is the application-facing row value. Its fields do not hold a connection, pool, transaction, global identity map or hidden service locator.
- Persistence belongs to an explicit SeaORM Entity/ActiveModel/query context. Operations receive a ConnectionTrait or DatabaseTransaction explicitly.
- SeaORM's ActiveModel uses per-field NotSet, Set and Unchanged values. This is a write-payload/change-state mechanism, not a process-wide Unit of Work or automatic identity map.
- A successful insert/update returns a model value built from values actually returned or read from the selected database. A failed operation must not mutate the caller's original Model value.
- Associations are explicit. Do not implement transparent lazy I/O or implicit relation fetches; query count and loading behavior remain visible to the caller.
- There is no application-global database singleton. Pool and transaction ownership remains explicit and request/test isolatable.

Although SeaORM places write methods on ActiveModel, its operation methods receive a connection argument. This does not bind a hidden connection to model instances and is accepted here as a Data Mapper-style architecture with explicit ActiveModel state.

### 2. Foundation: SeaORM 1.1.20, SQLx 0.8.6 and SeaQuery 0.32.x

Select **SeaORM 1.1.20** as the SQL ORM foundation. Do not add the dependency in 6.7.1: introduce the exact pin and server-only features in 6.7.2 together with the typed-pool migration, so Cargo.lock, dependency review and the one-pool connection contract land atomically.

Verified upstream manifest facts for SeaORM 1.1.20:

- Edition 2021, declared MSRV Rust 1.81, license MIT OR Apache-2.0.
- It depends on SQLx 0.8.4 or compatible 0.8.x, and SeaQuery 0.32.5 or compatible 0.32.x. The workspace's SQLx 0.8.6 satisfies the dependency range and should resolve to the shared SQLx 0.8.6 package in Cargo.lock.
- Its feature set provides SQLx drivers for SQLite, PostgreSQL and MySQL, Tokio/Rustls runtime support, model/entity derives and optional type integrations.
- Its 1.1.x driver documentation describes the sqlx-mysql feature as supporting MySQL and MariaDB, alongside PostgreSQL and SQLite.
- It supplies Entity/Model/ActiveModel derives, a composable query API, relation metadata, explicit transactions and relation loaders. The public loader API includes load_one, load_many and load_many_to_many.

The planned server-only SeaORM feature set for 6.7.2 is: macros; SQLx SQLite, PostgreSQL and MySQL; Tokio/Rustls; JSON; chrono; rust_decimal; bigdecimal; UUID; and time. When introduced, SeaORM must remain a dependency of rustflow-server only; it must not enter rustflow, rustflow-view, rustflow-dom, browser examples or the WASM dependency graph.

SeaORM uses SeaQuery internally for SQL AST construction and backend rendering. Do not introduce a second custom SQL renderer or duplicate query-builder implementation. Use the SeaQuery version resolved through SeaORM's 0.32.x dependency range; keep the lockfile reproducible and record the resolved version during the final dependency review.

Keep SQLx 0.8.6 as the execution/driver and migration foundation. Do not introduce another SQL driver or pool library. SQLx migrations remain the schema migration mechanism; SeaORM's schema-generation/migration tooling is not adopted in this ADR.

Do not enable SeaORM's mariadb-use-returning feature globally. SeaORM represents MySQL and MariaDB with the same MySQL backend family, so enabling a MariaDB-specific RETURNING path at compile time would also affect MySQL. For this adoption, leave the feature disabled and use SeaORM's non-RETURNING MySQL-family path until a runtime server-identity/capability strategy is separately designed and tested. Both MySQL 8.4.13 and MariaDB 11.4.13 require independent integration contracts before runtime support may be claimed.

### 3. Existing AnyPool boundary and typed-pool bridge

SeaORM 1.1.20 uses backend-specific SQLx pools (`SqlitePool`, `PgPool` and `MySqlPool`), not SQLx `AnyPool`. Its connector API can build a SeaORM `DatabaseConnection` from an existing typed SQLx pool handle. SQLx pool handles are cloneable references to the same underlying pool, which allows RustFlow to keep one pool lifecycle rather than creating a duplicate.

The Phase 6.7.2 implementation now follows this boundary: `DatabasePool` owns a backend-tagged enum containing exactly one typed SQLx pool; `DatabasePool::orm()` exposes the SeaORM connection built from that handle; the `sqlx_*_pool()` accessors expose only the configured driver's pool; and SQLx migrations dispatch to that same typed pool. `DatabasePool::begin_transaction()` uses SeaORM's transaction on the shared pool, with parameterized raw statements routed through the transaction's `ConnectionTrait` so ORM and raw operations can share one transaction. This intentionally replaces the pre-1.0 `AnyPool` and `AnyConnection` accessors.

The connection bridge is implementation work in progress, not acceptance evidence for the whole subphase. Phase 6.7.2 must still prove pool closure, limits, acquire timeouts, migrations, mixed ORM/raw transaction behavior, type mapping and CRUD on SQLite, PostgreSQL, MySQL and MariaDB. No duplicate pool, hidden pool or process-global connection is allowed. If the one-pool bridge fails any of these contracts, stop and amend this ADR rather than adding a second pool.

### 4. Model derive and metadata contract

Use SeaORM's maintained DeriveEntityModel for actual entity/model/column/primary-key code generation in 6.7.2. Phase 6.7.1 adds the RustFlow-owned ModelDiagnostics derive as a validation-only companion; the macro validates the declared SeaORM subset now but does not require or expand SeaORM itself:

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

The application places related entity declarations in their own Rust module, as in SeaORM's conventional entity layout.

ModelDiagnostics validates the supported SeaORM attribute subset before SeaORM generates code. It rejects unknown/duplicate entity and field attributes, malformed identifiers, non-named or generic model structs, duplicate column mappings, absent or nullable primary keys, nested Option, architecture-width integers, invalid generated-key combinations, unsupported portable field types and composite keys wider than SeaORM's supported arity. Explicit custom enum_name/column_type mappings require a matching SeaORM ValueType/ActiveEnum implementation. It does not generate SQL, model fields, entity traits, codecs or DDL.

The exact model naming, key, nullability, defaults, generated-value, type and field-state rules are in [ORM_MODEL_CONTRACT.md](../ORM_MODEL_CONTRACT.md). The derive deliberately accepts SeaORM's attribute vocabulary rather than inventing a second parallel model syntax.

### 5. Query builder, relations and loading are mandatory parts of the final ORM

SeaORM provides the maintained basis for query composition and relationship loading. RustFlow will not implement a separate general-purpose SQL AST or dialect renderer.

The final Phase 6.7 contract still requires RustFlow integration tests and documented wrappers for projections, aliases, nested AND/OR predicates, comparisons, NULL checks, IN, LIKE, ordering, pagination, joins, grouping/aggregates, HAVING, subqueries and engine-specific operations.

Relations must cover belongs-to/many-to-one, one-to-one, one-to-many and many-to-many. Loading must be explicit and support efficient batch/eager loading, including the documented nested-load scope. Use query-count tests to prevent N+1 regressions. Never silently perform network I/O when a relation field is accessed.

### 6. ActiveModel operations, hooks and transaction semantics

Use SeaORM's explicit ActiveModel semantics rather than creating an incompatible second unit-of-work model:

- NotSet means omit the field from an insert/update payload, allowing a database default or generated value to apply where supported.
- Set(value) means include the value in the write.
- Unchanged(value) records a value copied from a Model and is not a requested update; insert builders may still use it as an insert value.
- Update predicates use the complete primary key. RustFlow's eventual safe wrappers must validate missing keys and return a typed error rather than allowing a lower-level builder panic.
- Insert/update return the database-confirmed Model; delete returns affected-row information. The caller's original Model/ActiveModel is not mutated in place.
- Database-generated fields/defaults are never guessed. If the backend cannot return a generated value directly, any follow-up read must be explicit and tested.

SeaORM supplies opt-in ActiveModelBehavior hooks. Their order is:

1. before_save runs before insert/update.
2. after_save runs only after the write succeeded and a Model was returned.
3. before_delete runs before delete.
4. after_delete runs after delete succeeded.

Hooks receive the same explicit ConnectionTrait argument and may return an error. An after-hook error does not undo an already committed statement. Callers must enclose hook-plus-write sequences in an explicit transaction whenever atomicity across the full operation is required. RustFlow must document this partial-failure case and must not imply that a hook error means the database statement was rolled back.

Transactions are explicit. A callback error causes rollback through SeaORM's transaction helper; success attempts commit. Commit failure is reported, not converted to success. A transport loss during commit may leave the server outcome unknown; the API must not falsely promise that rollback occurred. Engine-specific DDL implicit commits, especially MySQL/MariaDB, are not made atomic by a client transaction wrapper.

### 7. Raw SQL escape hatch and safety

Keep an explicit raw SQL escape hatch for engine-specific operations. Prefer a Statement constructed from SQL plus bound values, or a typed SQLx pool/connection obtained from the one shared pool. Use the server's placeholder syntax. Caller values are always bound parameters; do not interpolate them into SQL text.

Dynamic identifiers and SQL fragments must be constructed from validated typed metadata/expressions, not untrusted strings. Unprepared SQL is only for trusted static migration/administrative SQL and never for request data.

Errors must distinguish validation/metadata, encode/decode, not-found, constraint, connection/pool, query and transaction failures. Public error messages and Debug representations must not reveal URLs, passwords, bound secrets or raw driver messages.

### 8. Strict SQL ORM / MongoDB ODM boundary

This SQL ORM covers only SQLite, PostgreSQL, MySQL and MariaDB. It uses relational metadata, SQL query expressions, SQL transactions and the existing engine-specific SQLx migration strategy.

MongoDB remains the separate document adapter introduced in Phase 6.6.2. Its independent Phase 6.7.3 ODM is implemented in `main` through PR #60 (final PR head `659f1750d611e57315263216167d43375da23574`; merge commit `7ffb89b5ca36017a428cf3f828b68f590f8d404b`, 2026-10-31). It owns BSON/document models, `_id` mapping, serializers/deserializers, collection/index lifecycle, document filters/updates, and MongoDB-native session/transaction limitations. No SQL model derive, relational query builder, SQL relationship or SQL migration behavior is shared with MongoDB by implication.

## Alternatives compared

| Alternative | Current verified strengths | Compatibility and material limitations | Decision |
|---|---|---|---|
| Custom Active Record | Simple CRUD can appear as methods on a model. | Couples model code to persistence; tempts hidden connection selection and implicit I/O; makes per-request/per-test database isolation and transaction ownership less clear. RustFlow would also have to implement derives, codecs, query building, relation loaders and dialect behavior itself. | Reject as the architecture. Use Data Mapper with explicit context. |
| SeaORM 2.0.4 | Current SeaORM major line; entity derives, CRUD, query building, relationships and SQLx-backed drivers. MIT OR Apache-2.0. | Official manifest declares Rust 1.94 and SQLx 0.9.0. Both conflict with RustFlow's Rust 1.85 MSRV and SQLx 0.8.6 baseline. | Reject for the current compatibility contract. Reassess only through a separate MSRV/dependency ADR. |
| SeaORM 1.1.20 | Full async ORM, model/entity derives, ActiveModel semantics, query composition, relationships/loaders, transactions and SQLx 0.8.x support. Official manifest declares Rust 1.81 and MIT OR Apache-2.0. Its 1.1.x docs list SQLx MySQL (including MariaDB), PostgreSQL and SQLite. | Uses backend-specific SQLx pools rather than the current AnyPool; a single-pool connection-boundary migration is required. Exact MariaDB 11.4.13 behavior is not proven by upstream feature documentation and must be tested in RustFlow CI. The 1.1 line is older than current 2.x. | **Selected foundation**, pinned at 1.1.20 for the current MSRV. |
| Diesel 2.3.14 + diesel-async 0.9.2 | Mature compile-time ORM/query builder; schema, associations and migrations. Diesel lists PostgreSQL, MySQL and SQLite and its repository README also mentions MariaDB. diesel-async provides async connections. MIT OR Apache-2.0. | Diesel 2.3.14 declares Rust 1.86, above RustFlow's 1.85 MSRV. diesel-async 0.9.2 uses Diesel 2.3.9 and separate tokio-postgres/mysql_async/SQLite connection choices, not SQLx 0.8.6. It would require a new driver, pool and migration direction. Its async docs list PostgreSQL/MySQL/SQLite; exact MariaDB 11.4.13 support is not enough to assume this existing contract. | Reject under present MSRV and SQLx continuity requirements. |
| Toasty 0.11.0 | New async ORM with separate drivers, model macros, query API and relations; MIT. | Current workspace manifest declares Rust 1.95 and uses SQLx 0.9.0. Its MariaDB driver documentation targets MariaDB 11.8+, not RustFlow's 11.4.13 target. Both MSRV and exact server contract conflict. | Reject for current compatibility contract. |
| SQLx 0.8.6 alone | Existing async execution, pools, parameter binding, migrations, SQLite/PostgreSQL/MySQL-protocol service contracts. | SQLx is a driver/query execution framework, not the complete model/relation/query abstraction RustFlow needs. Model derives, ActiveModel state and relation loaders would still need to be built. AnyPool is not a universal value-codec guarantee. | Retain as SeaORM's execution/driver and migration layer, not as the ORM itself. |
| SeaQuery 0.32.7 + SQLx 0.8.6 with RustFlow-owned ORM | A typed SQL AST and renderers for MySQL, PostgreSQL and SQLite; compatible with SQLx 0.8.x. MIT OR Apache-2.0. | It does not provide a complete ORM model/ActiveModel layer, relation metadata/loaders or full transaction/error contract. A custom layer would duplicate large portions SeaORM already supplies. The SeaQuery Any binder is not accepted as a universal codec; it has unsupported/panicking conversions for some types in the 0.32 line. | Not selected as a direct foundation. SeaORM already uses SeaQuery internally. |

### Security and dependency advisory review (snapshot: 2026-10-30)

Phase 6.7.2 adds SeaORM to the resolved dependency graph. `Cargo.lock` has been regenerated by Cargo 1.85 and committed with SeaORM 1.1.20, SeaQuery, and the complete selected SQLx driver graph. A locked `cargo audit` now runs in CI; its single explicit exception is documented and owned below. This section records the current security review of the resulting graph, not a claim that every advisory has been fixed.

- **RUSTSEC-2024-0363 (SQLx binary-protocol length truncation):** fixed in SQLx `>=0.8.1`; RustFlow locks SQLx `0.8.6`, so this advisory is outside the affected range. The advisory says MySQL and SQLite do not appear exploitable, while recommending the patched release for every backend. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0363.html); [SQLx 0.8.1 changelog](https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md#081---2024-08-23).
- **RUSTSEC-2026-0009 (time RFC 2822 parser stack exhaustion):** fixed in `time >=0.3.47`; the current lockfile resolves `time 0.3.55`, outside the vulnerable range. Keep this floor in the lockfile when optional time integrations are enabled. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0009.html).
- **RUSTSEC-2023-0071 (rsa Marvin timing side-channel):** the regenerated lockfile resolves `sqlx-mysql 0.8.6 -> rsa 0.9.10`. RustSec still lists no patched `rsa` release as of 2026-10-30. The dependency is inherited from the existing MySQL-protocol driver; it is not introduced by SeaORM, and no fixed upstream release is currently available. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html); [sqlx-mysql 0.8.6 dependency/advisory page](https://deps.rs/crate/sqlx-mysql/0.8.6).
- **Reachability qualification:** SQLx 0.8.6's MySQL client authentication source imports `RsaPublicKey` and uses `RsaPublicKey::encrypt` for the password exchange when the relevant authentication plugin is used without TLS; the TLS path sends the password inside the TLS stream instead. That client code path performs public-key encryption, not RSA private-key decryption/signing. The RustSec advisory concerns timing leakage from private-key operations. Consequently, the presence of the flagged transitive crate is a real dependency-review finding, but the cited SQLx client path alone is not evidence that RustFlow executes the vulnerable private-key operation. Do not call the advisory fixed, suppress it without an owner, or infer that TLS is configured merely because the crate enables TLS support. Review actual authentication/TLS configuration and the remote server's RSA handling before expanding production MySQL/MariaDB guarantees. [SQLx 0.8.6 authentication source](https://docs.rs/crate/sqlx-mysql/0.8.6/source/src/connection/auth.rs).
- **6.7.2 dependency gate:** Cargo.lock is now generated and committed from the workspace manifest with Rust 1.85; it contains SeaORM 1.1.20 and the selected four-engine SQLx graph. CI runs `cargo audit --deny warnings` using `.cargo/audit.toml`, in addition to GitHub Dependency Review. The sole RustSec exception is RUSTSEC-2023-0071 and is narrowly scoped to the inherited RSA dependency described above; all other advisories, yanked crates and audit warnings remain blocking. Revisit this exception whenever the SQLx MySQL dependency, authentication path or TLS policy changes; remove it when the flagged dependency is eliminated or a fixed release is adopted. A passing audit therefore means every non-excepted finding passed, not that the RSA advisory was fixed.

### Evidence versus inference

- **Verified from official manifests:** SeaORM 1.1.20 declares Rust 1.81, edition 2021, MIT OR Apache-2.0, SQLx 0.8.4-compatible dependency range and SeaQuery 0.32.5-compatible dependency range. SeaORM 2.0.4 declares Rust 1.94 and SQLx 0.9.0. Diesel 2.3.14 declares Rust 1.86. Toasty's current 0.11.0 workspace manifest declares Rust 1.95 and SQLx 0.9.0.
- **Verified from official driver docs/source:** SeaORM 1.1.x lists sqlx-mysql (MySQL and MariaDB), sqlx-postgres and sqlx-sqlite. SeaORM's DatabaseConnection has typed SQLx pool accessors. SeaORM's ConnectionTrait and TransactionTrait require explicit connection/transaction values; its loader API exposes load_one/load_many/load_many_to_many. Its MariaDB RETURNING feature is compile-time and shares the MySQL backend flag.
- **Verified at the Phase 6.7.1 decision point:** the workspace pinned SQLx 0.8.6, exposed AnyPool and used SQLx's migration runner. MySQL and MariaDB used the same MySQL protocol backend family. MongoDB was a separate driver and adapter. PR #59 replaces that historical AnyPool boundary with typed SQLx pools shared with SeaORM; this ADR's design snapshot should not be read as a description of the current branch API.
- **Design inference:** SeaORM 1.1.20 is a better maintained foundation than a RustFlow-authored ORM because it already provides the model, query, transaction and relation abstractions required by the maturity gate while staying inside the current MSRV and SQLx major/minor boundary. The typed-pool bridge is necessary because AnyPool is not accepted directly by SeaORM.
- **Phase 6.7.1 boundary (historical):** at the time this ADR was accepted, SeaORM 1.1.20 end-to-end behavior against RustFlow's exact service matrix, runtime codecs, CRUD through DatabasePool, relation query counts and the generated application were not yet verified. PR #59 subsequently implemented the SQL runtime contract; its integer-boundary, relationship-loading/mutation and live PostgreSQL COMMIT fault tests passed, together with all eight required CI checks, on head `2cadce73c19c37c2f088f572ea37a83c65c72380` (2026-10-31). Phase 6.7.2 was merged into `main` on 2026-10-31 at `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. The generated application and advanced ORM maturity work remain separate Phase 6.7 work. The independent MongoDB ODM was merged into `main` through PR #60 on 2026-10-31 and remains architecturally separate from the SQL ORM.

## Capability matrix: final maturity target and 6.7.1 boundary

| Capability | Mandatory final Phase 6.7 contract | 6.7.1 scope |
|---|---|---|
| Model/entity derives | SeaORM entities, Model and ActiveModel with useful RustFlow compile-time diagnostics | Validation derive and positive/compile-fail tests; no executable RustFlow persistence |
| Keys and metadata | Names, primary key(s), nullable/default/generated fields, indexes/constraints, relations | Contract and diagnostic subset; SQL DDL stays in migrations |
| Type mapping | Checked codec matrix per engine; reject overflow/loss and NULL incompatibility | Intended per-engine matrix; runtime codec tests pending |
| CRUD/bulk | Insert/update/delete/read and semantically supported bulk operations | Operation semantics defined; integration pending |
| Query builder | Typed projections/aliases, predicates, joins, grouping, pagination, subqueries and dialect capabilities | SeaORM selected; execution through RustFlow's shared pool pending |
| Relations/loaders | belongs-to, one-to-one, one-to-many, many-to-many; explicit eager/batch/nested loading; query-count tests | Foundation confirmed; RustFlow integration/tests pending |
| Transactions/errors | Explicit transaction context, rollback and uncertain-commit semantics, safe error mapping | Semantics specified; typed-pool bridge and integration pending |
| Hooks | SeaORM hooks documented, explicit connection and transaction composition | Hook order/partial failure contract specified |
| Raw SQL | Explicit parameterized escape hatch and engine-specific SQL | Statement/typed-pool strategy defined; no untrusted interpolation |
| Migrations | Existing SQLx migrations, dialect-specific DDL and honest non-atomic DDL limitations | Existing migration strategy retained; typed pool adaptation in 6.7.2 |
| MongoDB ODM | Separate BSON/document contract and verified MongoDB-native behavior | Explicitly outside SQL ORM; 6.7.3 |

### Phase 6.7.2 implementation snapshot — PR #59

The 6.7.2 implementation now places SeaORM 1.1.20 inside `rustflow-server`, shares one typed SQLx pool per configured backend with SeaORM, retains backend-specific SQLx accessors/migrations, and exposes borrowed `EntityRepository` values. The repository provides insert/update/save/delete/find and bulk builders; typed SeaORM query composition is used for filters, projections, aliases, joins, grouping, aggregates and subqueries. Explicit loaders cover belongs-to, has-one, has-many and many-to-many metadata, with no transparent lazy loading. ORM and parameterized raw SQL can use the same explicit transaction. `DatabaseError` maps stable categories and its `Debug` representation reports only the category, not the underlying driver/SQL text.

The accepted 6.7.2 contract exercises these paths across SQLite, PostgreSQL, MySQL and MariaDB with CI service variables required when `CI=true`. In addition to parents without related rows and all four relation kinds, the final suite covers signed integer boundaries, dialect-supported unsigned widths and overflow, nested-loader query counts, junction attach/update/detach/cascade rollback, and a live PostgreSQL backend termination during `COMMIT`. These tests passed on final PR #59 head `2cadce73c19c37c2f088f572ea37a83c65c72380`; all eight required checks passed, and PR #59 was merged into `main` as `8d387943d5f6ccb4d3f33ba3708311442e238c5f`.

## Failure and transaction semantics

- A successful insert/update returns values confirmed by the database or by a follow-up read. RustFlow does not invent database defaults or generated timestamps.
- A failed write does not return a success-shaped model or mutate the caller's original Model value.
- An after_save/after_delete hook can fail after the SQL statement succeeded. Outside an explicit transaction, the database change may already be committed; this is a partial failure, not an automatic rollback guarantee.
- A multi-step operation that must be atomic receives an explicit transaction. A callback error triggers rollback; a successful callback attempts commit.
- A failed commit is not success. Connection loss during commit can leave the server outcome unknown and requires reconciliation/idempotency.
- MySQL/MariaDB DDL may implicitly commit. The ORM cannot promise that a failed DDL migration has been fully rolled back by a transaction wrapper.
- Runtime errors must distinguish metadata/validation, encode/decode, not found, constraints, pool/connection, query and transaction outcomes. Public HTTP errors and Debug output must redact credentials/URLs and avoid raw parameter disclosure.

## Phase boundaries and exit gates

- **6.7.1:** this ADR and the Data Mapper/foundation decision; model/metadata/type/operation/hook/transaction contract; validation-only ModelDiagnostics derive; positive and compile-fail tests; technical and public documentation updates; all relevant required checks green on the final PR SHA. The SeaORM dependency/facade and typed pool are introduced together in 6.7.2. This phase does not integrate CRUD into RustFlow's legacy AnyPool or close Phase 6.7.
- **6.7.2:** typed-pool bridge to SeaORM without a second pool; migration and raw-SQL accessor evolution; checked runtime type codecs; query execution; CRUD/bulk operations; per-engine error classification; explicit transaction composition; real-service tests on the four SQL engines.
- **6.7.3:** MongoDB's separate BSON document mapper/ODM, including native IDs, queries, update operators, indexes and MongoDB-specific transaction/session semantics.
- **6.7.4:** relational associations, relation mutations, explicit eager/batch/nested loading, query-count and concurrency tests. Delivered and accepted in PR #62; see [ADR 0022](0022-advanced-sql-relation-loading.md) for the detailed API, tested limits and acceptance evidence.
- **6.7.5:** CLI/generated application and complete developer docs/examples against the accepted public APIs.
- **6.7.6:** final dependency/security, Rust 1.85/stable, compile-fail, runtime integration, WASM/browser, website, governance and quality maturity gate.

Phase 6.7 cannot close until the generated application demonstrates the complete ORM contract on SQLite, PostgreSQL, MySQL and MariaDB and the separate ODM contract on MongoDB. A working CRUD slice alone is insufficient.

## Upstream references

- SeaORM 1.1.20 [manifest](https://docs.rs/crate/sea-orm/1.1.20/source/Cargo.toml.orig), [1.1.x driver/runtime documentation](https://www.sea-ql.org/SeaORM/zh-CN/docs/1.1.x/install-and-config/database-and-async-runtime/), [typed SQLx pool accessors](https://github.com/SeaQL/sea-orm/blob/1.1.20/src/database/db_connection.rs), [active model and hooks](https://github.com/SeaQL/sea-orm/blob/1.1.20/src/entity/active_model.rs), [loader API](https://github.com/SeaQL/sea-orm/blob/1.1.20/src/query/loader.rs).
- SeaORM 2.0.4 [manifest/MSRV](https://docs.rs/crate/sea-orm/2.0.4/source/Cargo.toml).
- Diesel 2.3.14 [manifest/MSRV](https://docs.rs/crate/diesel/2.3.14/source/Cargo.toml), [relations documentation](https://diesel.rs/guides/relations.html); diesel-async 0.9.2 [manifest/MSRV and dependency set](https://docs.rs/crate/diesel-async/0.9.2/source/Cargo.toml.orig).
- Toasty [workspace manifest](https://github.com/tokio-rs/toasty/blob/main/Cargo.toml), [driver source](https://github.com/tokio-rs/toasty/tree/main/crates/toasty-driver-mysql), [MariaDB guide](https://github.com/tokio-rs/toasty/blob/main/docs/guide/src/mysql.md).
- SQLx 0.8.6 [crate documentation](https://docs.rs/crate/sqlx/0.8.6).
- RustFlow [database support matrix](../DATABASE_SUPPORT_MATRIX.md), [roadmap](../../ROADMAP.md), [existing SQLx database code](../../crates/rustflow-server/src/database.rs), and [MongoDB adapter contract](./0018-mongodb-document-adapter.md).