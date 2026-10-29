# ADR 0018: MongoDB document adapter

- Status: Implemented on PR #55; final acceptance depends on the required checks for the final PR head.
- Phase: 6.6.2 — Document database: MongoDB.
- Related: ADR 0013 (configuration contract), ADR 0016 (SQLx database pools and migrations), ADR 0017 (MySQL/MariaDB driver).

## Context

RustFlow currently exposes experimental SQLx pools and SQL migrations for relational engines. MongoDB stores BSON documents and uses collections, native query/update operators, and indexes. Treating MongoDB as another SQL dialect would blur incompatible guarantees and encourage invalid use of SQL transactions, placeholders, or migration tooling.

## Decision

- Use the official MongoDB Rust driver, pinned to 3.6.0. The selected release declares Rust 1.85 as its MSRV and is Apache-2.0 licensed. The exact pin, minimal feature selection, and lockfile make resolution reproducible. MongoDB 7.0.45 was selected as the explicit CI server patch (the latest 7.0 patch listed for October 13, 2026) instead of the 8.0 image line because the GitHub-hosted runner's kernel caused the 8.0 image to refuse startup before CI tests could run. The optional DNS SRV resolver feature is intentionally disabled: it pulled hickory-proto 0.25.2, which triggered high-severity advisory GHSA-3v94-mw7p-v465. This release supports standard mongodb:// connection strings; mongodb+srv:// is explicitly rejected until the DNS resolver dependency can be upgraded to a verified fixed version compatible with the driver and MSRV. Review driver security advisories during dependency review; an upgrade that raises the MSRV must be coordinated through the workspace policy rather than silently changing RustFlow's minimum version.
- Keep the adapter in rustflow-server::document_database, separately from database.rs, SQLx AnyPool, DatabaseBackend, SQL transaction helpers, and SQL migrations.
- Expose a small experimental boundary: MongoConfig, MongoDatabase, native typed Collection<T> handles, run_with_timeout, BSON conversion helpers, DocumentBootstrap, DocumentIndex, and MongoError.
- Keep collection operations native to MongoDB. Do not add an ODM, model derives, a cross-engine query builder, or portable relational semantics in Phase 6.6.2.
- Configure the URI and database separately using RUSTFLOW_MONGODB_URI and RUSTFLOW_MONGODB_DATABASE. Production uses the existing AppConfig::setting process-environment policy; the adapter does not load dotenv files independently. Redact the URI in Debug and do not return driver details in public HTTP messages.
- Default connect, server-selection, and wrapped operation timeouts to 10, 10, and 15 seconds. The driver has native connect/server-selection limits; RustFlow wraps driver action futures with the configured operation timeout. Consumers should use run_with_timeout for every operation whose overall duration must be bounded.
- Use BSON's serde integration through helpers that preserve serialization/deserialization errors as error sources. Expose the native BSON module through rustflow_server::mongodb_driver::bson.
- Define a repeatable bootstrap declaration for one collection and explicit indexes. The bootstrap creates the collection when absent, creates declared indexes, verifies names, and propagates errors. Unchanged declarations are intended to be idempotent. No automatic drop, index replacement, data migration, or SQL migration reuse is provided.
- Map HTTP 409 only when the MongoDB driver reports command code 11000 (duplicate key). Connection and operation timeouts/connectivity use a safe unavailable response; other internal errors produce a generic 500. The original driver error remains available through Error::source, while Display and Debug omit its message and credentials.

## Alternatives considered

- SQLx AnyPool: rejected because MongoDB is not an SQL engine and the driver cannot be forced into SQLx's relational contracts.
- Reuse SQL migrations: rejected because collection/index lifecycle and BSON documents are not SQL DDL migrations.
- Add an ODM in this phase: rejected because the roadmap reserves a separate mature document mapper/ODM contract for Phase 6.7.3.
- Use the latest driver release regardless of MSRV: rejected because this phase must retain Rust 1.85 compatibility; the chosen exact version will be re-evaluated when the project's MSRV policy changes.
- Implement an independent connection pool: rejected because the official driver owns and manages its client connection pools.

## Verification and CI

CI provisions a disposable authenticated mongo:7.0.45 service, passes RUSTFLOW_TEST_MONGODB_URI to integration tests, and runs the explicit mongodb_adapter integration target under Rust 1.85 and stable. The contract checks connection and server version, redaction, bounded failure to connect, authentication failure where URI credentials are present, CRUD, nested/optional BSON, unique indexes, duplicate-key classification, repeatable bootstrap, and resource cleanup. When run locally without RUSTFLOW_TEST_MONGODB_URI, the integration target may skip with an explicit message; in CI, a missing URI fails the test.

A workflow definition is not evidence that its check passed. Record the exact final PR head and the actual Rust, documentation, website, governance, dependency review, WASM, browser E2E/bundle and MongoDB integration results before marking this ADR or the roadmap criteria accepted.

## Consequences and limitations

- The adapter is experimental and MongoDB-specific. It does not imply compatibility with other server versions than the one actually tested.
- Client cancellation after the operation timeout does not guarantee that a server-side operation has no side effects; MongoDB's own timeouts, retry and write-concern semantics remain driver/server-specific.
- The bootstrap contract only creates missing collections/indexes and verifies index names. It does not reconcile an existing index's full key/options definition or manage versions of document shape.
- The adapter provides no ODM, generated models, query builder, SQL-like relationships, cross-engine transaction abstraction, or MongoDB schema migrations. Phase 6.6.3 and Phase 6.7 remain separate roadmap work.
