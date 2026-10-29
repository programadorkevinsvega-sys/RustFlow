# ADR 0020: SQL ORM architecture and model contract

- Status: Proposed for review with Phase 6.7.1.
- Phase: 6.7.1 — Model contract and architecture.
- Related: ADR 0016 (SQLx pools and migrations), ADR 0017 (MySQL/MariaDB driver), ADR 0018 (MongoDB document adapter), ADR 0019 (database support matrix), [ORM model contract](../ORM_MODEL_CONTRACT.md).

## Context

RustFlow's Phase 6.6 support contract is verified for bundled SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS and MongoDB 7.0.45 on the workspace's Rust 1.85 MSRV and stable. SQLx 0.8.6 currently provides a runtime-generic AnyPool, explicit transaction wrappers and SQLx migrations. The MongoDB adapter uses the official MongoDB Rust driver 3.6.0 and native BSON/collection/index APIs independently of SQLx.

No SQL ORM, model derive, model-row decoder, query builder or relationship loader is currently implemented. The CLI's generated models are structs; they do not imply persistence. Phase 6.7 must be a cohesive ORM and must not be closed as CRUD-only.

A full ORM candidate can conflict with the Rust 1.85 MSRV or current pool boundary. A shared SQL protocol/runtime-generic driver also does not make SQL types, placeholders, generated values, DDL or transaction guarantees identical. MongoDB must stay a separate document system.

## Decision

### 1. Pattern: Data Mapper

Use **Data Mapper**, not Active Record.

- Model structs contain application data and compile-time table/column metadata; they do not hold a hidden connection or execute persistence methods.
- Persistence operations belong to an explicit database/session/query context and receive a connection or transaction explicitly.
- No process-global database singleton, implicit lazy I/O, mutable global identity map, automatic lifecycle hooks or baseline Unit of Work.
- A successful write that returns generated values yields a new model value. A failed operation must not mutate the caller's in-memory model.
- Dirty tracking and Unit of Work are excluded from the baseline contract. Explicit transactions compose operations. Including these later requires a reviewed ADR amendment and tests.

This separates data from persistence and leaves space for efficient batch relation loading without hidden N+1 queries.

### 2. Execution and SQL dialect foundation

- Retain SQLx **0.8.6** as the SQL execution, connection and migration foundation. Do not write database drivers or a custom migration runner.
- Use SeaQuery's typed SQL AST and engine-specific renderer, with **SeaQuery 0.32.7** as the first compatible candidate line. Its official manifest declares Rust 1.62, MIT OR Apache-2.0, and MySQL/PostgreSQL/SQLite dialect builders. MySQL-family SQL is the starting dialect for MariaDB, but MariaDB-specific behavior still requires independent integration evidence.
- Values must be bound parameters, not concatenated SQL strings. Identifiers come from validated model metadata and are quoted by the renderer. Raw SQL remains an explicit escape hatch with parameters.
- The existing DatabasePool exposes SQLx AnyPool. It remains unchanged in 6.7.1. Before query execution/type codecs are implemented in 6.7.2, prove compatibility between query-builder values and the current pool. SQLx Any is not proof of full type portability.
- Do **not** use the SeaQuery 0.32.7 Any binder as the universal model-value codec. Its published source panics for UUID and JSON values, panics for decimal values, and unwraps unsigned 64-bit conversion to i64. Those behaviors violate a no-panic/range-check contract. Use the SeaQuery AST/renderer with checked, engine-aware value codecs/execution adapters.
- If typed codecs cannot correctly use AnyPool, 6.7.2 must introduce a backend-tagged SQLx connection boundary and a documented pre-1.0 migration for the raw-pool accessor before enabling ORM CRUD. It must not silently open a second pool for the same URL as a workaround.

### 3. Model metadata and derive

- Implement a small RustFlow-owned derive macro named Model for metadata only; do not copy another ORM's query/CRUD implementation.
- The derive accepts named-field, non-generic structs; table/column names; primary-key fields; nullable fields represented by Option<T>; database-generated/default markers; and explicit read-only mode for keyless views.
- It emits immutable metadata only. It does not execute SQL, implement codecs, generate DDL/migrations or imply runtime type support.
- Composite primary keys are supported in metadata. Keyless models must explicitly use the read-only attribute. Generated/default metadata is declarative; insert payload semantics and retrieval of generated values are implemented in a later subphase.
- Known Rust types map to logical categories. Unsupported field types, unknown/duplicate attributes, conflicting markers, invalid identifiers, missing primary keys and nullable keys are compile-time errors. Custom enum fields need an explicit sql_type = "enum" marker until typed enum mapping exists.

### 4. SQL ORM / MongoDB ODM boundary

The SQL ORM covers only SQLite, PostgreSQL, MySQL and MariaDB. It uses relational metadata, SQL query expressions, SQL transactions and engine-specific migration directories. MongoDB remains the separate document adapter introduced in Phase 6.6.2; its future ODM receives its own BSON/document contract, _id semantics, collection/index lifecycle and native query/update API in 6.7.3. No SQL model derive, relational query builder or migration semantics are shared with MongoDB by implication.

## Alternatives compared

| Option | Verified strengths and compatibility | Blockers / trade-offs | Decision |
|---|---|---|---|
| Active Record (custom model methods) | Convenient calls appear directly on entities and can make simple CRUD easy to discover. | Couples entities to persistence/session state, encourages hidden connection selection and implicit I/O, and complicates multiple databases per test/request. That conflicts with RustFlow's explicit transaction and isolation requirements and risks hidden N+1 loading. | Rejected as the public pattern; Data Mapper keeps model values free of persistence state. |
|---|---|---|---|
| SeaORM 2.0.4 | Mature ORM with entity derives, CRUD, query API and relations; MIT OR Apache-2.0. Official manifest: [docs.rs](https://docs.rs/crate/sea-orm/2.0.4/source/Cargo.toml). | Declares Rust 1.94 and SQLx 0.9.0, incompatible with this workspace's Rust 1.85 and SQLx 0.8.6 pins. | Rejected unless a separate ADR raises the MSRV and changes the driver contract. |
| SeaORM 1.1.20 | Mature entity/query/relation features. The [1.1.x driver docs](https://www.sea-ql.org/SeaORM/zh-CN/docs/1.1.x/install-and-config/database-and-async-runtime/) list SQLx MySQL (including MariaDB), PostgreSQL and SQLite; its MSRV is below 1.85 and it supports SQLx 0.8. MIT OR Apache-2.0. | Older major than current 2.x and uses per-engine SQLx pools rather than the current AnyPool, so direct reuse needs a connection-boundary refactor. | Not adopted as a second ORM/pool; retained as a behavior reference. |
| Diesel 2.3.14 | Mature compile-time ORM/query builder and migrations. Official manifest: [docs.rs](https://docs.rs/crate/diesel/2.3.14/source/Cargo.toml). | Declares Rust 1.86, above MSRV 1.85. Native client/async adapter choices do not fit the current SQLx/Tokio pool without a deliberate driver change. | Rejected unless the workspace MSRV and driver direction change by ADR. |
| Toasty 0.11.0 | New ORM with relations and separate driver crates. Official [release history](https://github.com/tokio-rs/toasty/releases). | Current manifest declares Rust 1.95 and SQLx 0.9-family components; it does not establish RustFlow's exact MariaDB 11.4.13 support contract. MIT. | Rejected for MSRV and unverified MariaDB contract. |
| SQLx 0.8.6 alone | Fits the workspace; supplies async execution, pools, parameters and migrations. [Official crate docs](https://docs.rs/crate/sqlx/0.8.6). | SQLx expressly is not an ORM; model metadata, relationship loading and cohesive query API remain to be defined. Any does not imply full type parity. | Retained as execution/driver layer, not the ORM itself. |
| SeaQuery 0.32.7 + SQLx 0.8.6 | [Manifest](https://github.com/SeaQL/sea-query/blob/0.32.7/Cargo.toml) declares Rust 1.62, MIT OR Apache-2.0, and MySQL/PostgreSQL/SQLite dialect builders. The [binder manifest](https://github.com/SeaQL/sea-query/blob/0.32.7/sea-query-binder/Cargo.toml) supports SQLx 0.8 and sqlx-any. | The Any binder has gaps/panics for UUID, Decimal, JSON and out-of-range u64; it is not a universal safe codec. Per-engine conversions must be tested and checked. | **Selected query AST/rendering foundation**, with checked RustFlow execution/value adapters and an integration proof before CRUD. |

### Evidence versus inference

- **Verified from official manifests:** listed release versions, declared Rust versions, license expressions, SQLx major/minor requirements and SeaQuery backend feature declarations. These facts do not imply benchmark performance.
- **Verified from source code:** the SeaQuery 0.32.7 Any binder's handling of UUID, Decimal, JSON and u64 in [sqlx_any.rs](https://github.com/SeaQL/sea-query/blob/0.32.7/sea-query-binder/src/sqlx_any.rs).
- **Design inference:** an ORM cannot safely treat runtime-generic Any as proof that all model value types are portable.
- **Security/advisory review (2026-10-29):** SQLx RUSTSEC-2024-0363 is patched in `>=0.8.1`, so the existing pinned SQLx 0.8.6 is outside the affected range ([RustSec](https://rustsec.org/advisories/RUSTSEC-2024-0363.html)). SeaQuery 1.0.2 is the current latest stable line but its official manifest declares Rust 1.88, so it is incompatible with RustFlow's 1.85 MSRV; the compatible 0.32.7 line remains a dependency/maintenance trade-off ([1.0.2 manifest](https://github.com/SeaQL/sea-query/blob/1.0.2/Cargo.toml)). SeaQuery 0.32.7's `with-time` integration allows `time ^0.3.36`; RUSTSEC-2026-0009 affects `time >=0.3.6,<0.3.47` and is fixed in `>=0.3.47` ([RustSec advisory](https://github.com/rustsec/advisory-db/blob/main/crates/time/RUSTSEC-2026-0009.md)). The existing lockfile uses `time 0.3.55`. The future SeaQuery integration must keep the lock resolved to a patched `time` version if enabling that feature; it must rerun dependency review and audit the complete enabled feature tree. The currently successful dependency-review workflow is evidence for this PR's dependency diff, not a claim that every future optional feature has been audited.

## Contract matrix: capabilities and exclusions

| Capability | Required final Phase 6.7 contract | 6.7.1 boundary |
|---|---|---|
| Model/table/column metadata | Typed metadata, safe names, primary key(s), nullable/default/generated/read-only | Metadata trait and derive; no SQL execution |
| Type mapping | Checked codecs with per-engine matrix and no silent loss | Logical categories/specification only; runtime codecs pending |
| CRUD and bulk operations | Insert/update/delete/read, generated values, affected rows and conflicts | Semantics specified; APIs/execution pending |
| Query builder | Projections, aliases, nested predicates, NULL/IN/LIKE, ordering/pagination, joins/group/HAVING/subqueries as in the roadmap | SeaQuery selected; runtime API pending |
| Relationships/loading | belongs-to, one-to-one, one-to-many, many-to-many; explicit eager/batch/nested loading and query-count tests | Contract boundary specified; implementation pending |
| Transactions/concurrency | Explicit context, rollback/error behavior, engine-specific atomicity; no hidden singleton | Semantics specified; integration pending |
| Errors/security | Typed validation/encode/decode/connection/constraint/concurrency errors; no secrets; parameters bound | Compile-time model errors in 6.7.1; DB errors pending |
| Lifecycle hooks / Unit of Work | Not in baseline; no implicit I/O or hidden dirty tracking | Explicitly excluded |
| SQL migrations | Existing SQLx migration boundary; dialect-specific DDL and no blanket MySQL-family atomicity promise | Existing behavior retained; ORM schema generation excluded |
| MongoDB ODM | Separate BSON/document/_id/index/query contract | Out of SQL ORM scope; planned as 6.7.3 |

## Failure and transaction semantics

- A persistence command does not mutate a model in place. Successful writes return values with only confirmed generated fields; failures return errors and no success-shaped model.
- A single statement outside an explicit transaction gets only the atomicity guaranteed by that server/statement. Multi-step operations requiring atomicity must receive an explicit transaction.
- A failed commit is never reported as success. Network failure during commit can mean an unknown server outcome; callers need reconciliation/idempotency rather than a false rollback guarantee.
- MySQL/MariaDB DDL can implicitly commit; the ORM must not promise migration DDL rolls back because it was issued inside a transaction wrapper.
- Ordinary updates are last-write-wins unless an opt-in optimistic-lock/version contract is added and tested.
- Runtime errors must distinguish model/metadata, value encode/decode, not found, constraint, pool/connection, query, concurrency and transaction failures. Public responses/Debug output must redact URLs and credentials and avoid raw SQL/value dumps.

## Phase boundaries and exit gates

- **6.7.1:** ADR, metadata contract, per-engine type matrix, derive diagnostics, failure semantics, positive/compile-fail tests, docs/site updates and green required checks on the final PR SHA. It does not deliver executable ORM/query operations.
- **6.7.2:** typed mapping/decoding, query execution integration, checked binding, CRUD/bulk operations, SeaQuery integration, per-engine error classification and explicit transaction composition. The AnyPool compatibility proof/connection-boundary migration is a gate, not optional follow-up work.
- **6.7.3:** separate MongoDB ODM using BSON and native collection/document semantics.
- **6.7.4:** relations and explicit eager/batch/nested loading with query-count/concurrency tests.
- **6.7.5–6.7.6:** generated application and final quality/maturity gate. Phase 6.7 cannot close until the whole contract passes on four SQL engines and the separate MongoDB ODM contract.

## Upstream references

- [SeaORM 2.0.4 manifest/MSRV](https://docs.rs/crate/sea-orm/2.0.4/source/Cargo.toml) and [SeaORM 1.1.x driver docs](https://www.sea-ql.org/SeaORM/zh-CN/docs/1.1.x/install-and-config/database-and-async-runtime/).
- [Diesel 2.3.14 manifest/MSRV](https://docs.rs/crate/diesel/2.3.14/source/Cargo.toml).
- [Toasty manifest](https://github.com/tokio-rs/toasty/blob/main/Cargo.toml) and [release history](https://github.com/tokio-rs/toasty/releases).
- [SQLx 0.8.6](https://docs.rs/crate/sqlx/0.8.6) and [SQLx placeholder docs](https://docs.rs/sqlx/0.8.6/sqlx/fn.query.html).
- [SeaQuery 0.32.7 manifest](https://github.com/SeaQL/sea-query/blob/0.32.7/Cargo.toml), [SeaQuery 1.0.2 manifest/MSRV](https://github.com/SeaQL/sea-query/blob/1.0.2/Cargo.toml), [binder manifest](https://github.com/SeaQL/sea-query/blob/0.32.7/sea-query-binder/Cargo.toml), [Any binder source](https://github.com/SeaQL/sea-query/blob/0.32.7/sea-query-binder/src/sqlx_any.rs), [SQLx RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0363.html), [time RustSec advisory](https://github.com/rustsec/advisory-db/blob/main/crates/time/RUSTSEC-2026-0009.md).
- [RustFlow support matrix](../DATABASE_SUPPORT_MATRIX.md) and [roadmap](../../ROADMAP.md).
