# Engineering baseline

This document defines the initial repository conventions. It is intentionally small and may evolve through reviewed changes.

The original Rust 1.85 baseline below is historical. ADR 0024 proposes Rust 1.94 as the current workspace MSRV to support SeaORM 2.0.4; migration verification is pending on GitHub CI.

## Workspace and crate naming

- The root `Cargo.toml` is a virtual workspace manifest; package code lives under `crates/`.
- Crate and package names use lowercase kebab-case. The initial public package is `rustflow`.
- Add a crate only when it has a cohesive responsibility, a meaningful test boundary, and a clear dependency direction. Do not split crates merely to mirror a diagram.
- Prefer workspace-inherited edition, MSRV, license, repository metadata, and lint policy.
- `crates/rustflow-cli` owns the `rustflow` binary and Cargo orchestration. Generators use deterministic templates and reject file collisions.
- The initial MSRV was Rust 1.85. ADR 0024 records the proposed increase to 1.94, including manifest, CI, generated-project, and documentation updates.

## Dependency policy

- Reuse maintained ecosystem infrastructure; do not reimplement async runtimes, HTTP servers, database drivers, or cryptographic primitives.
- Add dependencies only for a concrete requirement. Explain alternatives and trade-offs in the PR; use an ADR for major architectural dependencies.
- Check compatibility with the current workspace MSRV and all supported targets before adding a dependency.
- Keep browser/WASM dependencies out of server-only crates and server-only dependencies out of browser builds.
- Prefer narrowly scoped dependencies and features. Disable default features when appropriate and justified.
- Review license, maintenance health, security advisories, transitive dependency impact, and feature-tree growth.
- Commit Cargo.lock for reproducible development and CI. Revisit lockfile policy explicitly before publishing libraries.
- Do not add a dependency solely for functionality already provided by the standard library without a clear benefit.

## Quality gates

The historical CI baseline ran on Rust 1.85 and stable. ADR 0024 changes the configured lanes to Rust 1.94 and stable; a configured lane is not evidence that the new dependency line passed. A separate dependency-review workflow evaluates pull-request dependency changes. Dependabot proposes updates; it does not approve or merge them automatically.

A workflow file is not proof that a check passed. Report actual CI results and keep branch-protection required-check settings aligned with the check names shown by GitHub.

## Security and review

- Never commit secrets or real environment files.
- The experimental rustflow_server::AppConfig reads only RUSTFLOW_ENV, RUSTFLOW_HOST, and RUSTFLOW_PORT. Development/test dotenv files are parsed without mutating the process environment; production ignores dotenv files and requires process-level bind settings.
- The experimental server boundary supports typed input validation, `CsrfProtection`, server-side sessions and pre-handler authentication/authorization guards. Phase 6.4 is merged into `main`, but these APIs remain experimental. `InMemorySessionStore` is volatile and cannot coordinate multiple processes. The application must verify credentials and assign roles from trusted data. `CsrfProtection` must remain attached deliberately to cookie-authenticated mutations, including login/logout, and does not replace authorization.
- Phase 6.5 introduces an experimental SQLx pool/migration boundary for SQLite and PostgreSQL. SQL remains explicit and parameterized; no ORM or generic repository abstraction is implied. Production deployments must coordinate migrations before rolling out application replicas.
- Phase 6.6's historical matrix used SQLx 0.8.6 and Rust 1.85/stable for SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS and MongoDB 7.0.45. ADR 0024 proposes the new SQLx/SeaORM line and Rust 1.94; its checks remain pending. Keep shared SQL-driver guarantees distinct from engine-specific SQL, DDL, migration and transaction semantics. Phase 6.7 is complete and integrated into `main`. PR #66 closed the global ORM/ODM quality and compatibility gate and was squash-merged on 2026-11-01 as `6046ae7ca60843d145476177e698162b57939c68`, building on the accepted SQL ORM (PR #59), MongoDB ODM (PR #60), advanced relation/loading behavior (PR #62), and generated CRUD application (PR #64). The SQL ORM and separate MongoDB ODM remain experimental and pre-1.0. See `ROADMAP.md`, `DATABASES.md`, and `ORM_MODEL_CONTRACT.md` for accepted historical contracts and current limitations.
- Treat external input as untrusted and escape HTML by default when rendering is implemented.
- Keep changes focused, add tests for behavior and failure paths, and update docs when contracts change.
- Mark roadmap items complete only after evidence is available.
