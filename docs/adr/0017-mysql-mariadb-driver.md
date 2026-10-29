# ADR 0017: MySQL protocol driver for MySQL and MariaDB

- Status: Proposed for review with Phase 6.6.1.
- Phase: 6.6.1 — Relational engines: MySQL and MariaDB.
- Related: ADR 0013 (configuration), ADR 0016 (SQLx pools and migrations).

## Context

Phase 6.5 introduced SQLx 0.8.6, `AnyPool`, configuration validation, parameterized queries, transactions, error mapping, and versioned migrations for SQLite/PostgreSQL. RustFlow must extend that contract while retaining Rust 1.85 and avoiding a parallel connection abstraction.

## Decision

- Retain the pinned SQLx 0.8.6 Tokio/Rustls runtime and `AnyPool`; enable SQLx's `mysql` feature instead of adding a second driver.
- Use the SQLx MySQL protocol driver for both MySQL and MariaDB. `DatabaseBackend::MySql` identifies the driver family; the `mysql://` URL cannot reliably identify which server product responds.
- Accept `mysql://` URLs for both. Do not accept `mariadb://` for `AnyPool` driver selection. Parse MySQL URLs during configuration and keep the URL out of `Debug` and public error messages.
- Configure disposable MySQL 8.0 and MariaDB 11.4 services separately in CI and run both real-engine contracts on Rust 1.85 and stable. These are the initial intended verification versions; compatibility with other server versions is not implied until verified.
- Require MySQL-family migration directories to include `.rustflow-dialect` containing `mysql`. Existing SQLite/PostgreSQL migration directories without markers remain compatible; any present marker must match the selected backend.
- Keep SQL explicit and parameterized; do not introduce an ORM, dialect-rewriting layer, or MongoDB contract in this phase. Use separate migration directories when schema syntax or vendor-specific features diverge.
- Map MySQL/MariaDB unique violations through SQLx's backend-neutral `DatabaseError::is_unique_violation()` classification (the server's duplicate-entry number is 1062) to the existing safe HTTP conflict response while preserving the original SQLx error as the error source.

## Alternatives considered

- A separate MariaDB driver: rejected because SQLx's MySQL protocol driver supports the shared protocol family and a second adapter would duplicate pool/query/transaction infrastructure without an established need.
- Infer the MariaDB server product from `mysql://`: rejected because the scheme identifies the SQLx driver, not the server product.
- Reuse SQLite/PostgreSQL migrations blindly: rejected because DDL, types, placeholders, and implicit-commit behavior differ.
- Add an ORM in this subphase: rejected because Phase 6.6 covers database adapters and dialect contracts; Phase 6.7 owns the ORM/ODM contract.
- Accept additional server versions based only on protocol similarity: rejected; test services are pinned explicitly.

## Consequences and limitations

The public API remains experimental. MySQL and MariaDB share one `DatabaseBackend::MySql` variant and SQLx driver. Applications must select the correct server URL, use the target dialect's SQL, and validate vendor-specific behavior. Some MySQL-family DDL implicitly commits; RustFlow does not promise full atomicity for failed DDL migrations. MongoDB and the Phase 6.7 ORM/ODM remain unimplemented.

## Verification gate

Do not accept this decision as implemented until CI shows that the MySQL and MariaDB contract tests actually ran and passed on both Rust 1.85 and stable, alongside formatting, Clippy, workspace tests, rustdoc, WASM, browser E2E/bundle size, website build, governance, and dependency review. The PR report must list checks and limitations without extrapolating a result from only one server.
