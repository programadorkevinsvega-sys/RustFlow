# Database support matrix (Phase 6.6.3)

**Verification evidence:** the runtime-version assertions and engine integration contracts passed on PR #57 code head `51ac4c212b3d6bf68483ee2ef8e28b5c3a75f421` under Rust 1.85 and current stable, along with the required CI workflows. The service targets are pinned in `.github/workflows/ci.yml`. This matrix describes the exact versions RustFlow verifies; it is not a promise that every version in the same major line, server fork, SQL dialect, cloud service, or deployment topology behaves identically.

## Verified engine/version matrix

| Engine | CI server/runtime contract | Rust driver | Project Rust compatibility | Connection/configuration | Verified contract and limits |
|---|---|---|---|---|---|
| SQLite | SQLite 3.46.0 bundled by `sqlx-sqlite 0.8.6` / locked `libsqlite3-sys 0.30.1` | SQLx 0.8.6 SQLite driver through `AnyPool` | Workspace MSRV Rust 1.85; full workspace runs on 1.85 and stable | `sqlite://rustflow.db?mode=rwc`, `sqlite::memory:` | In-memory/file CRUD, repeatable forward migrations, explicit reversible migration rollback, bound writes, transaction rollback, and generated CLI application smoke test. SQLite SQL and migration syntax remain SQLite-specific. |
| PostgreSQL | PostgreSQL 16.15 (pinned CI image `postgres:16.15`) | SQLx 0.8.6 PostgreSQL driver through `AnyPool` | Workspace MSRV Rust 1.85; full workspace and service contract run on 1.85 and stable | `postgres://` or `postgresql://` via `RUSTFLOW_DATABASE_URL` | CI fails if the service URL is missing; the test asserts server version 16.15 and exercises pool acquisition and transaction rollback. The generated CLI CRUD smoke test uses SQLite, not PostgreSQL. |
| MySQL | MySQL 8.4.13 LTS (pinned CI image `mysql:8.4.13`) | SQLx 0.8.6 MySQL-protocol driver through `AnyPool` | Workspace MSRV Rust 1.85; full workspace and service contract run on 1.85 and stable | `mysql://` via `RUSTFLOW_TEST_MYSQL_URL` in integration tests; `RUSTFLOW_DATABASE_URL` in applications | CI asserts the exact server version and tests CRUD with bound values, transactions, pool behavior, duplicate-key classification, migration failure/rollback expectations, and dialect configuration. MySQL 8.0 was the earlier Phase 6.6.1 target; the maintained 8.4 LTS line replaces it in the current matrix. |
| MariaDB | MariaDB 11.4.13 LTS (pinned CI image `mariadb:11.4.13`) | The same SQLx 0.8.6 MySQL-protocol driver through `AnyPool` | Workspace MSRV Rust 1.85; full workspace and service contract run on 1.85 and stable | `mysql://`; `mariadb://` is not accepted for SQLx driver selection | CI separately asserts MariaDB 11.4.13 identity and runs the SQL engine contract. Public `DatabaseBackend::MySql` denotes the shared driver family; it cannot identify the server product. MySQL-family DDL may implicitly commit. |
| MongoDB | MongoDB Server 7.0.45 (pinned CI image `mongo:7.0.45`) | Official MongoDB Rust driver `mongodb = 3.6.0` | Driver MSRV is Rust 1.85; the full workspace and MongoDB integration contract run on 1.85 and stable | `RUSTFLOW_MONGODB_URI=mongodb://...`, `RUSTFLOW_MONGODB_DATABASE`, plus the documented timeout variables | Separate document adapter; CI tests the real server, typed CRUD, BSON nested/optional values and ObjectId, unique-index conflicts, repeatable collection/index bootstrap, bounded connectivity failures, and safe credential/error handling. `mongodb+srv://` is explicitly unsupported while the optional DNS SRV resolver is disabled. It does not use SQLx, `AnyPool`, SQL transactions, or SQL migrations. |

### Runtime/MSRV interpretation

- RustFlow's workspace MSRV is **Rust 1.85** (`Cargo.toml`); it is the compatibility floor for the complete application, not the lowest upstream MSRV of every dependency considered separately.
- SQLx is pinned to **0.8.6**. SQLx documents the 0.8.x line's upstream MSRV as Rust 1.78; RustFlow deliberately gates the whole workspace at 1.85.
- The official MongoDB Rust driver is pinned to **3.6.0**, whose documented MSRV is Rust 1.85. Driver 3.7 raises the upstream MSRV to 1.88 and must not be upgraded without a deliberate workspace-MSRV decision.
- CI runs the full workspace on the `1.85` toolchain lane and `stable`. The resolved stable version moves over time; the workflow result for a given PR is the evidence for the exact compiler build used.
- SQLite uses the SQLite source selected by the locked SQLx / `libsqlite3-sys` dependency with the bundled feature; updating that dependency can change the SQLite patch version. Keep the lockfile committed and refresh this row and the runtime version assertion together when upgrading the SQLite source.

## Shared SQL surface versus engine-specific contracts

The relational engines share the experimental SQLx boundary: `DatabaseConfig`, `DatabasePool`, the `DatabaseBackend` selector, connection-pool bounds, explicit transactions, the migration runner, URL redaction, and safe HTTP error mapping. They do **not** share identical SQL syntax or identical server semantics.

Engine-specific obligations include:

- **URLs and backend identity.** SQLite uses `sqlite:`; PostgreSQL accepts `postgres://` and `postgresql://`; both MySQL and MariaDB use `mysql://` and map to `DatabaseBackend::MySql`. The scheme selects a driver; it does not prove the server vendor or version.
- **Placeholders, types, and DDL.** Queries must be parameterized, but SQL text, placeholder syntax, data types, identity/auto-increment definitions, indexes, and constraints require validation against the actual target engine. Do not copy SQLite DDL to another engine and infer portability from a successful compile.
- **Migrations.** Keep engine-specific migration directories where syntax or semantics diverge. MySQL-family directories require `.rustflow-dialect` containing `mysql`. Existing SQLite/PostgreSQL directories without a marker remain supported; if present, their marker must match `sqlite` or `postgres`. MySQL-family DDL may implicitly commit, so a failed migration is not promised to be fully atomic.
- **Errors and transactions.** Shared error types translate known cases to safe application errors; the underlying database still determines transaction behavior, constraint metadata, and server-specific failure details. Only recognized unique-key errors are mapped to HTTP 409.
- **Generated application.** The CLI scaffold defaults to SQLite and its generated CRUD/migration sample is SQLite-specific. The generated-app integration test verifies this default and runs the generated server against SQLite. Selecting a different SQL URL alone does not port its queries/migrations; change and test them for that target engine. The scaffold does not initialize MongoDB.
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

The CI workflow pins the service images listed above and runs SQL tests on both Rust 1.85 and stable. The exact successful PR run is the evidence for the actual commit and environment; this document must not be used to infer that a new dependency or an unrun workflow passed.

When changing a server target, SQLx/MongoDB driver, `libsqlite3-sys`, Cargo features, or workspace MSRV:

1. Update the pinned CI image or lockfile and the exact-version integration assertion.
2. Run formatting, Clippy with warnings denied, all workspace tests, the MySQL/MariaDB and MongoDB integration targets, generated-app CLI tests, rustdoc, WebAssembly, browser E2E/bundle size, documentation, governance, and dependency review.
3. Update this table, `docs/DATABASES.md`, ADR 0017 / ADR 0018 as applicable, the website's database guide/project/API status, generated scaffold text/tests, and `ROADMAP.md` based on actual results.
4. Do not expand compatibility claims to untested server lines or treat a shared protocol as identical SQL behavior.

## Upstream version references

- [SQLx 0.8.6 crate documentation](https://docs.rs/crate/sqlx/0.8.6) and [SQLite driver details](https://docs.rs/crate/sqlx-sqlite/0.8.6).
- [MongoDB Rust driver/server compatibility and MSRV table](https://www.mongodb.com/docs/drivers/compatibility/?driver-language=rust).
- [PostgreSQL versioning policy](https://www.postgresql.org/support/versioning/) and [PostgreSQL 16.15 release notes](https://www.postgresql.org/docs/16/release-16-15.html).
- [MySQL 8.4.13 release notes](https://dev.mysql.com/doc/relnotes/mysql/8.4/en/news-8-4-13.html); MySQL 8.0 release notes record that 8.0 reached end of life in April 2026.
- [MariaDB 11.4 release list](https://mariadb.org/mariadb/all-releases/) and [11.4.13 LTS release announcement](https://mariadb.org/mariadb-server-12-3-11-8-11-4-and-10-11-q3-2026-maintenance-releases-and-goodbye-10-6/).
