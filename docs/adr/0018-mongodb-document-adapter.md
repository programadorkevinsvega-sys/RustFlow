# ADR 0018: MongoDB document adapter

- Status: Accepted; merged into `main` in PR #55 as `e3ceb86d757bb7aa1a07e16a1e21b462adb69841` after all required final-head checks passed.
- Phase: 6.6.2 — Document database: MongoDB.
- Related: ADR 0013 (configuration contract), ADR 0016 (SQLx database pools and migrations), ADR 0017 (MySQL/MariaDB driver).

## Context

RustFlow currently exposes experimental SQLx pools and SQL migrations for relational engines. MongoDB stores BSON documents and uses collections, native query/update operators, and indexes. Treating MongoDB as another SQL dialect would blur incompatible guarantees and encourage invalid use of SQL transactions, placeholders, or migration tooling.

## Decision

- The initial implementation pinned MongoDB Rust driver 3.6.0 to preserve the then-current Rust 1.85 MSRV. The workspace now pins `mongodb` 3.10.1 (`Apache-2.0`, crate MSRV 1.88) and declares Rust 1.94. Keep the exact pin, minimal feature selection, and lockfile. MongoDB 7.0.45 remains the isolated CI server version. The driver's optional `dns-resolver` feature pulls `hickory-net`, `hickory-resolver`, and `hickory-proto` 0.26.x; those packages are absent from the current lockfile because the feature is disabled. `RUSTSEC-2026-0120` affects `hickory-net` through 0.26.0 and is patched in 0.26.1; the current Hickory 0.26 line is 0.26.3, licensed MIT OR Apache-2.0, with Rust 1.88 MSRV. The reported DNSSEC code path additionally requires a DNSSEC feature, which MongoDB's `dns-resolver` feature does not itself enable. Nevertheless, do not enable `mongodb+srv://` here: a separate implementation change must pin and audit the resolved graph, verify the DNS features, and test SRV behavior without relying on Atlas or secrets. Standard `mongodb://` remains the supported URI.
- Keep the adapter in rustflow-server::document_database, separately from database.rs, SQLx AnyPool, DatabaseBackend, SQL transaction helpers, and SQL migrations.
- Expose a small experimental boundary: MongoConfig, MongoDatabase, native typed Collection<T> handles, run_with_timeout, BSON conversion helpers, DocumentBootstrap, DocumentIndex, and MongoError.
- Keep collection operations native to MongoDB. Do not add an ODM, model derives, a cross-engine query builder, or portable relational semantics in Phase 6.6.2.
- Configure the URI and database separately using RUSTFLOW_MONGODB_URI and RUSTFLOW_MONGODB_DATABASE. Production uses the existing AppConfig::setting process-environment policy; the adapter does not load dotenv files independently. Redact the URI in Debug and do not return driver details in public HTTP messages.
- Default connect, server-selection, and wrapped operation timeouts to 10, 10, and 15 seconds. The driver has native connect/server-selection limits; RustFlow wraps driver action futures with the configured operation timeout. Consumers should use run_with_timeout for every operation whose overall duration must be bounded.
- Use BSON's serde integration through helpers that preserve serialization/deserialization errors as error sources. Expose the native BSON module through rustflow_server::mongodb_driver::bson.
- Define a repeatable bootstrap declaration for one collection and explicit indexes. The bootstrap creates the collection when absent, creates declared indexes, verifies names, and propagates errors. Unchanged declarations are intended to be idempotent. No automatic drop, index replacement, data migration, or SQL migration reuse is provided.
- Map HTTP 409 only when the MongoDB driver reports duplicate-key code 11000, whether surfaced as a command error or a write error. Connection and operation timeouts/connectivity use a safe unavailable response; other internal errors produce a generic 500. The original driver error remains available through Error::source, while Display and Debug omit its message and credentials.

## Alternatives considered

- SQLx AnyPool: rejected because MongoDB is not an SQL engine and the driver cannot be forced into SQLx's relational contracts.
- Reuse SQL migrations: rejected because collection/index lifecycle and BSON documents are not SQL DDL migrations.
- Add an ODM in this phase: rejected because Phase 6.6.2 was scoped to the low-level native document adapter; the separate mature document mapper/ODM contract was reserved for Phase 6.7.3 and subsequently implemented in main through PR #60.
- Enable SRV immediately after the Hickory fix: deferred. The evaluation confirms a patched Hickory release is available, but a separate implementation change is required to validate the lockfile resolution and resolver behavior without changing this adapter's feature boundary implicitly.
- Implement an independent connection pool: rejected because the official driver owns and manages its client connection pools.

## Verification and CI

CI provisions a disposable authenticated mongo:7.0.45 service, passes RUSTFLOW_TEST_MONGODB_URI to integration tests, and runs the explicit mongodb_adapter integration target under Rust 1.85 and stable. The contract checks connection and server version, redaction, bounded failure to connect, authentication failure where URI credentials are present, CRUD, nested/optional BSON, unique indexes, duplicate-key classification, repeatable bootstrap, and resource cleanup. When run locally without RUSTFLOW_TEST_MONGODB_URI, the integration target may skip with an explicit message; in CI, a missing URI fails the test.

Final verification evidence: PR #55 head `8c1a33c261769bd731f00c371bac17a569611c68` passed the CI workflow on Rust stable and Rust 1.85, including the explicit MongoDB 7.0.45 integration contract and MySQL 8.0/MariaDB 11.4 contracts. The documentation site, governance checks, dependency review, WebAssembly target check, and browser E2E/bundle workflow also passed for that exact head. The PR was merged into `main` as `e3ceb86d757bb7aa1a07e16a1e21b462adb69841`. Phase 6.6.3 and Phase 6.7 remain separate roadmap work.

## Consequences and limitations

- The adapter is experimental and MongoDB-specific. It does not imply compatibility with other server versions than the one actually tested.
- Client cancellation after the operation timeout does not guarantee that a server-side operation has no side effects; MongoDB's own timeouts, retry and write-concern semantics remain driver/server-specific.
- The bootstrap contract only creates missing collections/indexes and verifies index names. It does not reconcile an existing index's full key/options definition or manage versions of document shape.
- The Phase 6.6.2 adapter itself provided no ODM, generated models, query builder, SQL-like relationships, cross-engine transaction abstraction, or MongoDB schema migrations. Phase 6.7.3 subsequently added the independent trait-based ODM and merged it into `main` through PR #60; it does not change the limitations listed for this low-level adapter or add document migrations. Phase 6.6.3 is complete, while the global Phase 6.7 maturity gate remains open.
