# ADR 0016: SQLx, connection pools, and versioned migrations

- Status: Proposed for review with Phase 6.5.
- Phase: 6.5 — Database and migrations.
- Related: ADR 0013 (configuration), ADR 0014 (input validation/CSRF), ADR 0015 (sessions/access control).

## Context

RustFlow already has an experimental HTTP server, typed request validation, CSRF middleware and server-side session/access-control primitives. The CLI emits structural model and migration templates, but there is no SQL persistence layer. The database boundary must reuse maintained infrastructure without turning RustFlow into an ORM or leaking database dependencies into the WebAssembly runtime.

## Decision

- Integrate SQLx 0.8.6 with runtime Tokio/Rustls support, SQLite, PostgreSQL, the runtime-generic Any driver and SQLx migrations. MySQL was deferred to Phase 6.6.1; the proposed extension is recorded in ADR 0017.
- Expose configurable DatabaseConfig and cloneable DatabasePool from rustflow-server. DatabaseConfig validates supported URL schemes, pool bounds and acquire timeout; its Debug representation redacts the connection URL.
- Resolve RUSTFLOW_DATABASE_URL, max/min pool connections and acquire timeout using AppConfig precedence. Development/test may read dotenv files. Production reads database settings only from the process environment and requires an explicit database URL.
- Re-export SQLx for explicit, parameterized queries. Do not introduce an ORM or invent a model/query abstraction before CRUD use cases establish its shape.
- Provide an explicit transaction wrapper with access to the SQLx connection and consuming commit/rollback methods. Dropping an uncommitted transaction requests rollback.
- Use SQLx's versioned migration table and checksums. Apply pending migrations explicitly from a directory. Revert only when corresponding .down.sql migrations exist and the caller explicitly requests a target version.
- Translate common database outcomes into safe HTTP responses: row-not-found 404, recognized unique-constraint conflict 409, pool exhaustion 503 and otherwise generic 500. Do not expose SQL, driver errors or connection strings in response messages.
- Keep SQLite/PostgreSQL integration coverage, including repeatable migrations, rollback, parameterized writes and transaction behavior. Phase 6.6.1 proposes separate disposable MySQL and MariaDB CI services; acceptance is gated on both real-server contracts passing.
- Development/test scaffold startup can apply migrations. Production deployment runs migrations as a single ordered deployment step before changing application replicas; forward corrective migrations are preferred to risky production rollback.

## Alternatives considered

- A custom ORM/ActiveRecord layer: rejected because there is not yet enough validated CRUD use-case evidence.
- Build database drivers or transaction machinery in-house: rejected in favor of SQLx.
- Apply destructive down migrations automatically at application startup: rejected because a rollback may remove data or break concurrently deployed application versions.
- Treat all SQL and DDL as portable between engines: rejected; placeholders and schemas remain dialect-specific and must be tested.
- Add MySQL in the first slice: deferred to avoid increasing dependency and test scope before SQLite/PostgreSQL contracts are verified.

## Consequences and limitations

The API is experimental and couples RustFlow's public surface to SQLx's runtime query types. It is not an ORM, repository layer, migration CLI or general schema DSL. SQLite and PostgreSQL require engine-appropriate SQL. Database-backed sessions are not included; the existing in-memory session adapter remains process-local. Production migrations, pool sizing, backup/restore and rollout order remain deployment responsibilities.

## Verification gate

The generated CLI integration test builds a project using the RustFlow CLI and verifies validated CRUD, persistence, password verification, session/auth guards, CSRF, rendered pages and HTML escaping. SQLite and PostgreSQL receive CI coverage. Before merging, required checks must pass on the final PR head for Rust 1.85/stable, formatting, Clippy, workspace tests, rustdoc, WASM, browser E2E/bundle budget, documentation, governance and dependency review.
