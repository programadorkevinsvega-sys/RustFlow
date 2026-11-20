# Database support matrix

**Current stack verification:** PR #84 passed the required Rust, SQL engine, ORM, generated CRUD, dependency audit and documentation checks for the Rust 1.94 / SQLx 0.9.0 / SeaORM 2.0.4 / SQLite 3.54.1 stack. PR #85 passed the required checks after updating the MongoDB driver to 3.10.1. The matrix applies to these exact versions and PR heads; it is not a compatibility guarantee for other versions or deployments. Historical results cited below remain evidence for their original commits only.

The Phase 6.6 contracts passed on PR #57 code head `51ac4c212b3d6bf68483ee2ef8e28b5c3a75f421` under Rust 1.85 and stable. The Phase 6.7.6 compatibility gate passed on PR #66 HEAD `b049d96491e2b23bfe794ac3c0ed9abb68f7e493` (squash merge `6046ae7ca60843d145476177e698162b57939c68`) under Rust 1.85 and stable. The service targets remain pinned in `.github/workflows/ci.yml`. This matrix is not a promise that every version in the same major line, server fork, SQL dialect, cloud service, or deployment topology behaves identically.

## Configured engine/version matrix

| Engine | Configured CI server/runtime target | Rust driver | Project Rust compatibility | Connection/configuration | Configured checks and limits |
|---|---|---|---|---|---|
| SQLite | SQLite 3.54.1 bundled by locked `libsqlite3-sys 0.39.1` | SQLx 0.9.0 from pinned upstream commit `987b8c5a74b6da8b71da6b5549925a7f6e59aac9`, shared with SeaORM 2.0.4 | Workspace MSRV Rust 1.94; verified on PR #84 | `sqlite://rustflow.db?mode=rwc`, `sqlite::memory:` | PR #84 passed the bundled runtime assertion and in-memory/file CRUD, migrations, transaction, ORM, pool, and generated HTTP CRUD contracts. SQLite SQL and migration syntax remain SQLite-specific. |
| PostgreSQL | PostgreSQL 16.15 (pinned CI image `postgres:16.15`) | SQLx 0.9.0 from the pinned upstream commit, shared with SeaORM 2.0.4 | Workspace MSRV Rust 1.94; verified on PR #84 | `postgres://` or `postgresql://` via `RUSTFLOW_DATABASE_URL` | PR #84 passed server version assertion and ORM/codec/relation, pool, concurrency, migration, and generated HTTP CRUD contracts. |
| MySQL | MySQL 8.4.13 LTS (pinned CI image `mysql:8.4.13`) | SQLx 0.9.0 from the pinned upstream commit, shared with SeaORM 2.0.4 | Workspace MSRV Rust 1.94; verified on PR #84 | `mysql://` via `RUSTFLOW_TEST_MYSQL_URL` in integration tests; `RUSTFLOW_DATABASE_URL` in applications | PR #84 passed server version assertion and ORM CRUD/query/relation behavior, bound values, transaction rollback, pool handling, concurrency, duplicate-key classification, migrations, and generated HTTP CRUD. MySQL 8.0 was the earlier Phase 6.6.1 target; the maintained 8.4 LTS line replaces it. |
| MariaDB | MariaDB 11.4.13 LTS (pinned CI image `mariadb:11.4.13`) | SQLx 0.9.0 `MySqlPool` from the pinned upstream commit, shared with SeaORM 2.0.4 | Workspace MSRV Rust 1.94; verified on PR #84 | `mysql://`; `mariadb://` is not accepted for SQLx driver selection | PR #84 passed MariaDB identity assertion and ORM/query/relation, pool, concurrency, and generated HTTP CRUD contracts. `DatabaseBackend::MySql` denotes the shared driver family; it cannot identify the server product. MySQL-family DDL may implicitly commit. |
| MongoDB | MongoDB Server 7.0.45 (pinned CI image `mongo:7.0.45`) | Official MongoDB Rust driver `mongodb = 3.10.1` | Workspace MSRV is Rust 1.94 | `RUSTFLOW_MONGODB_URI=mongodb://...`, `RUSTFLOW_MONGODB_DATABASE`, plus the documented timeout variables | PR #85 passed the separate document adapter's real-server contract for typed CRUD, BSON nested/optional values and ObjectId, unique-index conflicts, repeatable collection/index bootstrap, bounded connectivity failures, and safe credential/error handling. `mongodb+srv://` remains unsupported while the optional DNS SRV resolver is disabled. It does not use SQLx, `AnyPool`, SQL transactions, or SQL migrations. |

### Runtime/MSRV interpretation

- RustFlow's configured workspace MSRV is **Rust 1.94** (`Cargo.toml`), required by the selected SeaORM 2.0.4 / SQLx 0.9 stack.
- SQL date/time codecs use SeaORM's `time` and Chrono integrations. Chrono is marked unmaintained with no patched release; `cargo audit` keeps a narrowly scoped exception for RUSTSEC-2026-0543, documented in ADR 0024.
- SQLx packages use version **0.9.0** from the exact upstream git revision in the workspace patch table. The published 0.9.0 release does not yet admit the patched SQLite crate; replace the patch only with a release that does.
- The official MongoDB Rust driver is pinned to **3.10.1**, verified with the integration update in PR #85; RustFlow's workspace MSRV remains 1.94 regardless of the driver's own minimum.
- The Rust 1.94 and stable lanes passed on the final SQL-stack PR #84 head; PR #85 passed after the MongoDB driver update. These results apply to those exact heads.
- SQLite uses bundled `libsqlite3-sys 0.39.1` / SQLite 3.54.1. The SQLite advisory addressed by ADR 0024 is no longer present in the locked graph. The current `cargo audit` exception is Chrono `RUSTSEC-2026-0543`; passing audit does not mean that Chrono advisory is fixed.

## Shared SQL surface versus engine-specific contracts

The relational engines share the experimental SQLx boundary: `DatabaseConfig`, `DatabasePool`, the `DatabaseBackend` selector, connection-pool bounds, explicit transactions, the migration runner, URL redaction, and safe HTTP error mapping. They do **not** share identical SQL syntax or identical server semantics.

Engine-specific obligations include:

- **URLs and backend identity.** SQLite uses `sqlite:`; PostgreSQL accepts `postgres://` and `postgresql://`; both MySQL and MariaDB use `mysql://` and map to `DatabaseBackend::MySql`. The scheme selects a driver; it does not prove the server vendor or version.
- **Placeholders, types, and DDL.** Queries must be parameterized, but SQL text, placeholder syntax, data types, identity/auto-increment definitions, indexes, and constraints require validation against the actual target engine. Do not copy SQLite DDL to another engine and infer portability from a successful compile.
- **Migrations.** Keep engine-specific migration directories where syntax or semantics diverge. MySQL-family directories require `.rustflow-dialect` containing `mysql`. Existing SQLite/PostgreSQL directories without a marker remain supported; if present, their marker must match `sqlite` or `postgres`. MySQL-family DDL may implicitly commit, so a failed migration is not promised to be fully atomic.
- **Errors and transactions.** Shared error types translate known cases to safe application errors; the underlying database still determines transaction behavior, constraint metadata, and server-specific failure details. Only recognized unique-key errors are mapped to HTTP 409.
- **Generated application.** The default server scaffold and `.env.example` are SQLite-first. The `crud` template provides engine-specific migration directories for SQLite, PostgreSQL and the MySQL protocol (used by both MySQL and MariaDB). Phase 6.7.5/6.7.6 generated-app integration exercises HTTP CRUD, the Category/Product relation, explicit relation loading, typed name/category filters and bounded pagination against SQLite, PostgreSQL, MySQL and MariaDB. This contract passed on PR #66's exact HEAD before merge. Selecting a different SQL URL alone does not port a user's own queries/migrations; adapt and test them for the target engine. The scaffold does not initialize MongoDB.
- **MongoDB.** MongoDB is a separate document contract with `MongoConfig`, `MongoDatabase`, the official driver's native typed collections, BSON helpers, operation timeout wrapping, and declarative collection/index bootstrap. The independent typed ODM from Phase 6.7.3 is now implemented in `main` over the same client; see [DATABASES.md](DATABASES.md) and [ADR 0021](adr/0021-mongodb-odm-architecture.md). This support matrix verifies engine/driver versions and its listed integration contract; it does not make the experimental ODM a stable 1.0 API. Bootstrap does not drop resources, reconcile every possible index option, or migrate arbitrary document shapes.

## Configuration examples

Use placeholders from your deployment secret manager; do not commit credential-bearing URLs.

```dotenv
# Default generated scaffold: SQLite
RUSTFLOW_DATABASE_URL=sqlite://rustflow.db?mode=rwc

# Alternative SQL targets; make queries and migration directories match the selected engine.
# RUSTFLOW_DATABASE_URL=postgres://<user>:<password>@<host>/<database>
# RUSTFLOW_DATABASE_URL=postgresql://<user>:<password>@<host>/<database>
# RUSTFLOW_DATABASE_URL=mysql://<user>:<password>@<host>/<database>
```

MongoDB uses a different API and configuration contract:

```dotenv
RUSTFLOW_MONGODB_URI=mongodb://<user>:<password>@<host>:27017/<database>?authSource=admin
RUSTFLOW_MONGODB_DATABASE=<database>
# Optional positive integer seconds:
# RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS=10
# RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS=10
# RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS=15
```

The application must explicitly construct/use `MongoConfig` and `MongoDatabase`; these variables do not replace `RUSTFLOW_DATABASE_URL`. Standard `mongodb://` is supported; SRV-style `mongodb+srv://` is rejected in this dependency configuration.

## Evidence and maintenance

The historical workflow ran SQL tests on Rust 1.85 and stable. The dependency migration and SQL contracts passed on the final PR #84 head; the MongoDB driver update passed on the final PR #85 head. A successful run is evidence only for its exact commit and environment; this document must not be used to infer that a later commit or an unrun workflow passed.

When changing a server target, SQLx/MongoDB driver, `libsqlite3-sys`, Cargo features, or workspace MSRV:

1. Update the pinned CI image or lockfile and the exact-version integration assertion.
2. Run formatting, Clippy with warnings denied, all workspace tests, the MySQL/MariaDB and MongoDB integration targets, generated-app CLI tests, rustdoc, WebAssembly, browser E2E/bundle size, documentation, governance, and dependency review.
3. Update this table, `docs/DATABASES.md`, ADR 0024 and earlier ADRs only to clarify supersession, the website's database guide/project/API status, generated scaffold text/tests, and `ROADMAP.md` based on actual results.
4. Do not expand compatibility claims to untested server lines or treat a shared protocol as identical SQL behavior.

## Upstream version references

- [SQLx upstream fix revision](https://github.com/transact-rs/sqlx/commit/987b8c5a74b6da8b71da6b5549925a7f6e59aac9) and the [RUSTSEC-2026-0459 advisory](https://rustsec.org/advisories/RUSTSEC-2026-0459.html).
- [MongoDB Rust driver/server compatibility and MSRV table](https://www.mongodb.com/docs/drivers/compatibility/?driver-language=rust).
- [PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/) and [PostgreSQL 16.15 release notes](https://www.postgresql.org/docs/16/release-16-15.html).
- [MySQL 8.4.13 release notes](https://dev.mysql.com/doc/relnotes/mysql/8.4/en/news-8-4-13.html); MySQL 8.0 release notes record that 8.0 reached end of life in April 2026.
- [MariaDB 11.4 release list](https://mariadb.org/mariadb/all-releases/) and [11.4.13 LTS release announcement](https://mariadb.org/mariadb-server-12-3-11-8-11-4-and-10-11-q3-2026-maintenance-releases-and-goodbye-10-6/).
