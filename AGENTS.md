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
6. Update docs and examples when behavior changes.
7. Run applicable checks.
8. Report exactly what changed, what ran, and what remains unverified.

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

## 8. Documentation and roadmap

- Keep docs aligned with implemented behavior, not only the intended design.
- Mark roadmap items complete only with evidence.
- Label illustrative snippets as proposed if they are not compilable.
- Update limitations and breaking-change notes.
- Do not claim production readiness while release gates remain open.

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
