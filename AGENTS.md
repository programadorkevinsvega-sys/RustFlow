# AGENTS.md — RustFlow Repository Instructions

These instructions apply to human contributors and coding agents. If a task conflicts with security policy or architecture rules, stop and ask for clarification rather than silently weakening a safeguard.

## 1. Read before changing code

For non-trivial work, read README.md, ROADMAP.md, ARCHITECTURE_RULES.md, SECURITY.md, relevant ADRs, crate docs, tests, and issue/PR discussion. Inspect actual code before claiming a feature exists or changing an API.

## 2. Workflow

1. Define the requested outcome and acceptance criteria.
2. Inspect relevant files and existing behavior.
3. Make the smallest coherent change.
4. Preserve dependency direction and security boundaries.
5. Add/update tests for normal, edge, and failure paths.
6. Before implementing a roadmap phase, inspect the planned changes and decide whether they require public documentation updates. Record the documentation impact in the phase/PR plan.
7. Update public docs and examples alongside the implementation whenever a developer-visible capability, API, behavior, workflow, limitation, or breaking change is introduced or changed.
8. Run applicable checks, including the documentation build when public docs or the website change.
9. Report exactly what changed, what ran, and what remains unverified.

A phase must not be considered ready for completion until its public-documentation impact has been reviewed and any required updates are included or explicitly justified as not applicable.

Never claim a test, build, browser check, scan, or benchmark passed unless it was actually run and observed.

## 3. Architecture

- Follow DDD, Clean Architecture, Hexagonal Architecture, and SOLID in ARCHITECTURE_RULES.md.
- Keep domain/application policy independent of Axum, Tokio, SQLx, browser APIs, CLI parsing, and filesystem details.
- Use narrow ports and adapters; avoid cyclic crate dependencies and catch-all modules.
- Keep the public facade thin and public APIs intentional.
- Avoid speculative abstractions and plugin systems without a demonstrated need.
- Add ADRs for decisions that change crate boundaries, reactivity, hydration, server/client boundaries, public APIs, or major dependencies.

## 4. Rust quality

- Use stable Rust unless a documented requirement says otherwise.
- Prefer safe Rust. Every new unsafe block needs a safety argument, focused tests, and review.
- Avoid unwrap/expect on runtime input, network data, environment configuration, and fallible I/O. Tests may use them when appropriate.
- Use typed errors and preserve useful context.
- Do not ignore errors or convert failures into success-shaped defaults.
- Consider async cancellation, timeout behavior, MSRV, feature flags, target support, and dependency licenses.
- Document public APIs, invariants, lifecycle behavior, and non-obvious trade-offs.

## 5. Reactivity and UI

- Keep reactivity semantics deterministic and documented.
- Test dependency tracking, dynamic dependencies, batching, re-entrancy, disposal, and cleanup.
- Ensure unmounted components dispose effects and event listeners.
- Keep DOM mutation in the renderer/DOM adapter.
- Do not promise thread safety the implementation does not provide.
- Keep browser-only dependencies out of server-only builds and vice versa.
- Escape text and attributes by default; raw HTML requires explicit API design and security review.
- Preserve keyed-list identity and test hydration behavior when implemented.
- Include semantic HTML and accessibility considerations.
- Treat the React-like component-authoring contract in `docs/adr/0004-react-like-component-authoring.md` and `ROADMAP.md` as a product requirement, not optional ergonomic polish.
- Do not claim `signal(value)`, `Signal::update`, `#[component]`, typed callback props, automatic `{signal}` interpolation, keyed reconciliation, or hydration exists unless the corresponding code/tests actually provide it.
- Do not close Phase 3 based only on successful WASM compilation. The intended Counter and todo examples must compile with the documented public API and pass browser E2E tests for state updates, DOM identity, callback props, and cleanup.
- Keep any proposed API examples clearly labeled as design targets until compile-tested. Never add a hidden process-global mutable runtime to simplify syntax without an explicit, reviewed ADR.

## 6. Backend and security

- Treat request data and client-serialized state as untrusted.
- Enforce authorization on the server for every protected operation.
- Use parameterized SQL and explicit transaction boundaries.
- Never implement cryptographic primitives or password hashing from scratch.
- Never log passwords, tokens, cookies, authorization headers, or sensitive bodies.
- Never commit real secrets. Use placeholders in .env.example.
- Apply request limits and timeouts to exposed endpoints.
- Do not weaken CSRF, cookie, escaping, authorization, or validation to make tests pass.
- Report vulnerabilities privately according to SECURITY.md.

## 7. Tests and verification

Run relevant narrow tests first, then broader checks where feasible. Once the workspace supports them, the expected baseline is:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
~~~

These commands may not run until the workspace and crates are implemented. If a command cannot run, explain why; never claim success.

For relevant changes, also run macro compile-fail tests, WASM/browser E2E tests, HTTP/database integration tests, dependency/secret scans, or benchmarks as appropriate. Avoid tests depending on uncontrolled external networks or wall-clock timing.

## 8. Public documentation and roadmap

- Treat the public developer documentation portal as part of the product and maintain it alongside implementation, not as a later cleanup task.
- Before starting each roadmap phase, inspect the planned implementation and explicitly assess whether it changes or adds anything that developers or technical teams need to know publicly. Include the documentation plan in the phase/PR scope.
- Before a phase is marked complete or work moves to the next phase, verify that all applicable public documentation updates are included. If no update is needed, state why in the PR/phase notes.
- For each developer-visible feature, document the relevant concepts, supported public API, intended use cases, tutorial or usage flow, accurate examples, limitations, failure modes, and relevant tests/verification.
- Keep the public documentation journey coherent: environment/tooling setup, first verifiable example, core concepts, reactive components and applications as they become available, and later server, SSR, routing, and CLI guides only when those capabilities actually exist.
- Public documentation is for RustFlow users: developers, software engineers, technical teams, and technology companies. Explain how to use supported capabilities, not how private internals work.
- Do not publish internal backend business logic, private system workflows, secrets, credentials, private operational details, internal-only implementation notes, or sensitive security mechanisms. Keep internal architecture and engineering notes in repository-internal docs/ADRs where appropriate; only expose the minimum public API contracts, integration behavior, and security guidance users need.
- Keep docs aligned with implemented behavior, not only intended design. Never invent APIs or imply planned capabilities are available.
- Mark roadmap items complete only with evidence.
- Label illustrative snippets as proposed if they are not compilable; prefer examples that can be compiled or verified.
- Update limitations and breaking-change/migration notes.
- Do not claim production readiness while release gates remain open.
- Run the website build for public documentation changes and ensure navigation/links are updated.

## 9. Dependencies

Before adding a dependency, check maintenance, security posture, license, MSRV, WASM/target compatibility, feature/compile-time impact, and whether it belongs in core, an adapter, build tooling, or dev-dependencies. Centralize versions in workspace configuration where practical.

## 10. Git hygiene

- Do not overwrite unrelated changes.
- Keep changes focused and commits descriptive.
- Do not commit generated build output, credentials, local environment files, or temporary debug output.
- Do not delete files or rewrite history without explicit authorization.
- Do not mark incomplete implementations production-ready.

## 11. Completion report

Report changed files and behavior, commands actually run and results, important trade-offs, remaining limitations, and roadmap/ADR updates. Be concise and factual.
