# ADR 0019: Public database support contract and verified engine matrix

- Status: Proposed for Phase 6.6.3; acceptance requires the final PR head to pass all required checks.
- Phase: 6.6.3 — Public support contract.
- Related: ADR 0016 (SQLx pools and migrations), ADR 0017 (MySQL/MariaDB driver), ADR 0018 (MongoDB document adapter), `docs/DATABASE_SUPPORT_MATRIX.md`.

## Context

RustFlow added relational drivers and a separate MongoDB document adapter across Phases 6.5 and 6.6.1/6.6.2. Those implementations used explicit test targets, but public documentation still used mixed historical versions and stale status statements. A package dependency or a workflow declaration alone is not evidence that an engine/version is verified. The public contract must name the actual CI engine versions, the Rust version floor of the complete workspace, the exact driver versions, and the specific tests that ran.

The earlier Phase 6.6.1 CI target was MySQL 8.0. The official MySQL release notes say the 8.0 line reached end of life in April 2026; Phase 6.6.3 moves the maintained support target to MySQL 8.4 LTS. This does not retroactively erase the historical Phase 6.6.1 result; it supersedes it as the current public CI target.

## Decision

- Maintain one canonical matrix in `docs/DATABASE_SUPPORT_MATRIX.md`, with links from the technical database guide and website reference pages.
- Gate the workspace at its documented MSRV of Rust 1.85, and run the full workspace on Rust 1.85 and stable. Driver-level upstream MSRV and RustFlow's full-workspace MSRV must be presented separately.
- Pin the CI test contract to PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS, SQLite 3.46.0 bundled via the locked SQLx SQLite/libsqlite3-sys dependency, and MongoDB 7.0.45.
- Keep SQLx 0.8.6 as the shared SQL driver package. The `mysql` feature is one MySQL-protocol driver for both MySQL and MariaDB. `DatabaseBackend::MySql` does not identify the server product. MySQL and MariaDB use `mysql://`, and `mariadb://` is not accepted for SQLx driver selection.
- Assert actual server/runtime versions in the reproducible CI contracts rather than inferring version coverage from service image declarations alone. CI must fail when a required PostgreSQL, MySQL, MariaDB, or MongoDB service URL is absent.
- Keep shared SQLx APIs distinct from engine-specific syntax and semantics. Pool configuration, explicit transaction handling, migration orchestration, safe error mapping, and URL redaction are shared boundaries; placeholders, types, DDL, indexes, unique violations, transaction semantics, and migrations must be tested for each target engine. MySQL-family DDL can implicitly commit.
- Keep MongoDB independent from SQLx: use `MongoConfig`, `MongoDatabase`, BSON and the native collection/index lifecycle. Do not present `RUSTFLOW_DATABASE_URL` or SQL migrations as MongoDB configuration. `mongodb = 3.6.0` is pinned because its MSRV aligns with Rust 1.85; `mongodb+srv://` remains explicitly unsupported while the optional DNS SRV resolver feature is disabled for dependency-security reasons.
- The generated CLI scaffold remains SQLite-first. Its generated SQL and migration sample are not claimed to be portable just because `RUSTFLOW_DATABASE_URL` can select another SQLx driver. The generated-app test must assert this default and the documentation that queries/migrations require engine-specific adaptation. The scaffold does not initialize MongoDB.
- A support row is accepted only after corresponding tests pass on the final commit, the documentation reflects that exact evidence, and all repository quality gates succeed. A docs-only matrix or a green workflow file without passing results is not acceptance evidence.

## Alternatives considered

- Claim all server versions supported because the wire protocol or driver is shared: rejected; only the explicit tested version targets are in the public matrix.
- Retain MySQL 8.0 as the current target after its documented end-of-life: rejected; use an active LTS server line and validate it in CI.
- Treat a common `AnyPool` as a promise of identical SQL: rejected; SQL syntax, migration expectations, constraints, and DDL transaction behavior are engine-specific.
- Force MongoDB into the SQLx matrix: rejected; the document driver, BSON semantics, collection bootstrap and indexes are a separate contract.
- Change the CLI scaffold to claim generic query/migration portability: rejected; scaffolded examples should remain honest about their SQLite-specific SQL and migrations.

## Verification plan

The Phase 6.6.3 PR must run:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- the explicit `database_engines` MySQL/MariaDB contract and `mongodb_adapter` contract
- generated CLI project tests, including the generated server/SQLite CRUD smoke test and scaffold configuration contract
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
- WebAssembly target check, browser E2E/bundle budget, docs site, governance, and dependency review

Record the exact passing PR HEAD in `ROADMAP.md`, this ADR, `docs/DATABASES.md`, and public docs. Do not check the Phase 6.6.3 criteria until this evidence exists.

## Upstream references

- [SQLx 0.8.6](https://docs.rs/crate/sqlx/0.8.6) and [SQLite driver details](https://docs.rs/crate/sqlx-sqlite/0.8.6).
- [MongoDB Rust driver compatibility and MSRV](https://www.mongodb.com/docs/drivers/compatibility/?driver-language=rust).
- [PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/).
- [MySQL 8.4.13 release notes](https://dev.mysql.com/doc/relnotes/mysql/8.4/en/news-8-4-13.html) and [MySQL 8.0 release notes](https://dev.mysql.com/doc/relnotes/mysql/8.0/en/).
- [MariaDB 11.4 release list](https://mariadb.org/mariadb/all-releases/).
