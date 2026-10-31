# ADR 0017: MySQL protocol driver for MySQL and MariaDB

- Status: Implemented and merged into `main` via PR #54. The original Phase 6.6.1 server targets were MySQL 8.0 and MariaDB 11.4; see ADR 0019 and `docs/DATABASE_SUPPORT_MATRIX.md` for the current public contract.
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

The public API remains experimental. MySQL and MariaDB share one `DatabaseBackend::MySql` variant and SQLx driver. Applications must select the correct server URL, use the target dialect's SQL, and validate vendor-specific behavior. Some MySQL-family DDL implicitly commits; RustFlow does not promise full atomicity for failed DDL migrations. At the time of this Phase 6.6.1 decision, MongoDB and the Phase 6.7 ORM/ODM were not yet implemented. Since then, Phase 6.6.2/6.6.3 completed MongoDB support and the separate SQL ORM/MongoDB ODM were added in Phase 6.7.2/6.7.3; see ROADMAP.md and DATABASES.md.

## Verification gate

The exact historical PR head `3e97b580cc86fdd3b0307724c54a45efe0a9209f` passed the MySQL 8.0 and MariaDB 11.4 contracts and migration-marker unit test on Rust 1.85 and stable, alongside the required quality gates. PR #54 was merged into `main` as `48a491d660c71005b58ed88a4f6a7e8ba3390c9d`. This ADR records the Phase 6.6.1 decision and its original CI evidence; Phase 6.6.3 supersedes MySQL 8.0 as the current public target with MySQL 8.4.13 LTS and MariaDB 11.4.13 LTS. The complete current matrix and evidence boundaries are in `docs/DATABASE_SUPPORT_MATRIX.md`.
