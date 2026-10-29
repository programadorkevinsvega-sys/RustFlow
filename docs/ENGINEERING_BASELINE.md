# Engineering baseline

This document defines the initial repository conventions. It is intentionally small and may evolve through reviewed changes.

## Workspace and crate naming

- The root `Cargo.toml` is a virtual workspace manifest; package code lives under `crates/`.
- Crate and package names use lowercase kebab-case. The initial public package is `rustflow`.
- Add a crate only when it has a cohesive responsibility, a meaningful test boundary, and a clear dependency direction. Do not split crates merely to mirror a diagram.
- Prefer workspace-inherited edition, MSRV, license, repository metadata, and lint policy.
- `crates/rustflow-cli` owns the `rustflow` binary and Cargo orchestration. Generators use deterministic templates and reject file collisions.
- The MSRV is Rust 1.85. CI checks both 1.85 and current stable. Raise it only through a documented decision and corresponding manifest, CI, and documentation updates.

## Dependency policy

- Reuse maintained ecosystem infrastructure; do not reimplement async runtimes, HTTP servers, database drivers, or cryptographic primitives.
- Add dependencies only for a concrete requirement. Explain alternatives and trade-offs in the PR; use an ADR for major architectural dependencies.
- Check compatibility with Rust 1.85 and all supported targets before adding a dependency.
- Keep browser/WASM dependencies out of server-only crates and server-only dependencies out of browser builds.
- Prefer narrowly scoped dependencies and features. Disable default features when appropriate and justified.
- Review license, maintenance health, security advisories, transitive dependency impact, and feature-tree growth.
- Commit Cargo.lock for reproducible development and CI. Revisit lockfile policy explicitly before publishing libraries.
- Do not add a dependency solely for functionality already provided by the standard library without a clear benefit.

## Quality gates

The CI workflow runs formatting, Clippy with warnings denied, tests, and documentation builds on Rust 1.85 and stable. A separate dependency-review workflow evaluates pull-request dependency changes. Dependabot proposes updates; it does not approve or merge them automatically.

A workflow file is not proof that a check passed. Report actual CI results and keep branch-protection required-check settings aligned with the check names shown by GitHub.

## Security and review

- Never commit secrets or real environment files.
- The experimental rustflow_server::AppConfig reads only RUSTFLOW_ENV, RUSTFLOW_HOST, and RUSTFLOW_PORT. Development/test dotenv files are parsed without mutating the process environment; production ignores dotenv files and requires process-level bind settings.
- The experimental server boundary supports typed input validation, `CsrfProtection`, server-side sessions and pre-handler authentication/authorization guards. Phase 6.4 is merged into `main`, but these APIs remain experimental. `InMemorySessionStore` is volatile and cannot coordinate multiple processes. The application must verify credentials and assign roles from trusted data. `CsrfProtection` must remain attached deliberately to cookie-authenticated mutations, including login/logout, and does not replace authorization.
- Phase 6.5 introduces an experimental SQLx pool/migration boundary for SQLite and PostgreSQL. SQL remains explicit and parameterized; no ORM or generic repository abstraction is implied. Production deployments must coordinate migrations before rolling out application replicas.
- Phase 6.6.1 uses SQLx 0.8.6's `mysql` feature as the shared MySQL-protocol driver for MySQL and MariaDB. PR #54 is merged; its historical real-service contracts were MySQL 8.0 and MariaDB 11.4. Phase 6.6.2 added a separate MongoDB 7.0.45 document adapter through PR #55; it does not use SQLx `AnyPool` or SQL migrations. Phase 6.6.3 publishes the current verified matrix in `docs/DATABASE_SUPPORT_MATRIX.md`: bundled SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS, and MongoDB 7.0.45, all gated by Rust 1.85/stable CI and exact server/runtime assertions. Keep shared SQLx API guarantees separate from engine-specific SQL, DDL, migration, and transaction semantics. Phase 6.7 remains planned for a mature SQL ORM and a separate mature MongoDB document mapper/ODM—not a CRUD-only MVP. An ADR must choose the architecture, proven foundations, and explicit feature contract before coding; dialect differences, efficient relation loading, type mapping, transaction/error semantics, diagnostics, and CI-backed integration evidence are required before Phase 6.7 can close.
- Treat external input as untrusted and escape HTML by default when rendering is implemented.
- Keep changes focused, add tests for behavior and failure paths, and update docs when contracts change.
- Mark roadmap items complete only after evidence is available.
