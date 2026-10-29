# RustFlow Product Scope (Phase 0.2)

**Status:** Accepted as the initial product-scope baseline  
**Phase:** 0.2 — Product scope  
**Audience:** maintainers, contributors, and users evaluating the project

## 1. Product statement

RustFlow is an early-stage Rust web framework project intended to make it practical to build interactive web interfaces and full-stack web applications with Rust. Its differentiator is a RustFlow-owned reactive UI model combined with a convention-oriented server workflow inspired by Laravel. The project is not production-ready and does not promise stable APIs.

The first milestone is a small, testable vertical slice—not a complete full-stack framework. RustFlow's target is a React-like component-authoring experience built with Rust types and RustFlow-owned reactivity; it is not a promise of source/API compatibility with React. Parts of this experience were not implemented when this baseline was accepted and must remain labeled as planned until verified.

## Current implementation note (2026-10-29)

This document preserves the original Phase 0.2 product-scope baseline; its references to the initial MVP and later roadmap milestones are historical scope statements, not the current feature inventory. Since this baseline, later phases have implemented the component/browser MVP, HTTP server prototype, SSR page/layout composition, opt-in DOM hydration, and opt-in client navigation/forms. Those additions do not make RustFlow stable or production-ready. For current availability and exact limits, see the [public project status](../website/src/content/docs/project-status.md), [API availability reference](../website/src/content/docs/reference/api-status.md), and [ROADMAP.md](../ROADMAP.md).

## 2. Target users

### Primary
- Rust developers who want to build interactive browser applications without writing the main UI logic in JavaScript.
- Full-stack Rust developers who value explicit types, testability, and a cohesive server/client development workflow.
- Framework contributors interested in reactive runtime semantics, component lifecycles, and maintainable crate boundaries.

### Secondary
- Teams evaluating Rust and WebAssembly for selected interactive UI workloads.
- Developers who want familiar conventions for routing, request handling, validation, and database integration.

### Not an initial target
- Teams that require a mature, stable, production-supported framework today.
- Applications requiring complete browser API coverage, a feature-complete ORM, or turnkey enterprise authentication in the first release.
- Developers seeking a drop-in replacement for React, Laravel, or an existing Rust framework. This is distinct from RustFlow's explicit goal of React-like declarative authoring and reactive UI behavior.

## 3. Product principles

1. **Prove a vertical slice first.** Prefer a working counter and todo app over broad, unintegrated abstractions.
2. **Own the differentiator.** RustFlow owns reactivity semantics and component lifecycle; reuse mature infrastructure for HTTP, async execution, database drivers, serialization, and cryptography.
3. **Explicit contracts.** Document update ordering, cleanup, rendering, errors, supported targets, and compatibility before treating behavior as stable.
4. **Safe by default.** Escape rendered text and attributes, keep secrets/server-only types out of client bundles, and use maintained security primitives.
5. **Testable boundaries.** Core behavior must be unit-testable without a browser or network; browser and server integrations receive dedicated tests.
6. **Accessible HTML.** Prefer semantic HTML, keyboard-accessible interactions, and correctly associated form labels.
7. **Honest maturity.** Do not claim features, compatibility, performance, or production readiness without evidence.
8. **Incremental decisions.** Architectural choices remain revisitable through documented ADRs and implementation evidence.
9. **React-like authoring is a product requirement.** Prefer a working, typed component API with local state, reactive interpolation, callbacks, and efficient DOM updates over isolated syntax that looks JSX-like but does not deliver reactive behavior.
10. **Familiar vocabulary, RustFlow semantics.** Expose recognizable names such as `use_state`, `use_effect`, `use_memo`, `use_ref`, and `use_context` in idiomatic `snake_case` when those capabilities are implemented. Do not imply exact React signatures or hook-order semantics; document the signal-first contract and actual availability.

## 4. Supported platforms and compatibility policy

### Initial development baseline
- Rust stable toolchain, with the repository's initial MSRV policy of Rust 1.85.
- Cargo workspaces and Linux/Ubuntu as the documented baseline developer environment.
- Native host builds for core logic and tests.
- The `wasm32-unknown-unknown` target for the browser runtime when Phase 3 begins.
- Current stable Chromium as the initial browser automation target, with Firefox and WebKit added as CI capacity and implementation maturity permit.

### Explicit limitations
- No guarantee of support for every browser or browser API.
- No promise that all crates compile for both native and WASM targets; target-specific dependencies must be separated.
- MSRV and browser support are policies to verify in CI, not claims that the pipeline already passes.
- Server deployment environments and database engines will be narrowed and documented when those integrations are implemented.

## 5. MVP definition

The MVP is complete only when all of the following are implemented, documented, and verified:

1. A typed Rust function component accepts typed props and owns reactive signal state through the intended ergonomic API.
2. Ordinary signal interpolation updates the affected DOM region automatically when state changes.
3. DOM event handlers and component callback props work and are cleaned up with component lifecycle disposal.
4. A todo example uses the same public API and supports add, toggle, remove, conditional rendering, and keyed list updates that preserve child identity.
5. A Rust server handles multiple routes and returns safely rendered HTML and JSON.
6. CI runs core unit tests, browser tests, and server integration tests on pull requests.
7. A clean-checkout guide lets a contributor install prerequisites, run checks, and launch examples.

This paragraph records the initial MVP boundary. Later phases have since added SSR page/layout composition and opt-in hydration/navigation, but RustFlow still does not provide a complete production-ready full-stack workflow or an integrated user/credential provider; it also lacks a custom ORM and production deployment tooling. Phase 6.4 adds experimental session and access-control primitives, not a complete account-authentication system.

## 6. MVP user stories and acceptance criteria

### US-1: Build a reactive component
**As a** Rust developer, **I want** typed signal state and effects, **so that** UI updates follow state changes predictably.

Acceptance criteria:
- A signal can be read and written using a documented API; the intended component-facing API provides ergonomic `signal(value)`, read, write, and update operations without requiring each component author to manually construct a Runtime.
- Signal ownership and runtime isolation are explicit, scope-bound, and tested; convenience must not rely on an undocumented process-global mutable runtime.
- An effect reruns when, and only when, a dependency it observed changes.
- Dynamic dependency changes stop tracking dependencies no longer read.
- Disposal prevents future scheduled work for the disposed scope.
- Unit tests cover normal updates, dynamic dependencies, and disposal.

### US-2: Render and interact with a component
**As a** application developer, **I want** to render a component and handle user events, **so that** I can build an interactive page.

Acceptance criteria:
- Text, elements, fragments, and nested components have documented representation.
- A normal signal child such as `{count}` inside `view!` updates automatically; users do not need an explicit deferred-child wrapper for routine signal interpolation.
- Components support typed props and typed callback props with compile-time diagnostics.
- Text and attribute values are escaped by default in server-rendered output.
- Event handlers are removed when their owning component is unmounted.
- Invalid or unsupported operations produce documented diagnostics rather than silent corruption.
- Browser tests cover interaction and cleanup.

### US-3: Build a todo example
**As a** learner, **I want** a small complete example, **so that** I can validate the framework's basic developer experience.

Acceptance criteria:
- Users can add, toggle, and remove todos.
- Empty and populated states render correctly.
- List items preserve identity under insertion, removal, and toggling.
- Form controls have labels and work with keyboard interaction.
- The example is runnable from a clean checkout using the intended ergonomic component API and is exercised by compile checks and browser E2E tests in CI.

### US-4: Serve application routes
**As a** full-stack Rust developer, **I want** typed route handlers and predictable responses, **so that** I can serve pages and API data.

Acceptance criteria:
- An example server registers at least two routes.
- Tests cover success responses, unknown routes, and JSON output.
- Request/response behavior and error mapping are documented.
- Request body limits and sensible timeout behavior are configured before accepting untrusted production traffic.
- Server integration tests run in CI.

### US-5: Contribute from a clean checkout
**As a** contributor, **I want** documented setup and automated checks, **so that** I can verify changes consistently.

Acceptance criteria:
- The guide lists required Rust targets and tools.
- CI checks formatting, Clippy, tests, documentation tests, and dependency advisories as the relevant stages are introduced.
- Pull requests receive actionable check results.
- README and roadmap status match actual evidence; unimplemented capabilities remain clearly labeled planned.

## 7. Non-goals for the first MVP

- Reimplementing Tokio, an HTTP server, database drivers, cryptographic primitives, or a general-purpose ORM.
- Supporting every HTML element, browser API, and browser engine from day one.
- Turnkey SSR/hydration and client-side navigation in the initial MVP; later phases implemented an opt-in prototype with explicit compatibility and fallback limits.
- Built-in production-grade authentication, authorization, sessions, and migrations.
- Hot reload, a complete design system, or an extensive component library.
- Promising API stability, performance superiority, or production readiness without release evidence.
- Hiding JavaScript/WASM/browser limitations behind claims of universal Rust-only execution.

## 8. Risk register

| Risk | Likelihood / impact | Mitigation | Evidence / trigger |
| --- | --- | --- | --- |
| Reactive graph leaks or stale dependencies | High / High | Specify semantics before implementation; test dynamic dependencies, disposal, cycles, and re-entrancy | Deterministic regression tests and memory/lifecycle checks |
| Procedural macro complexity and poor diagnostics | High / Medium | Start with a constrained grammar; use compile-fail tests and source spans | Invalid syntax produces actionable errors |
| Ergonomic component API hides runtime ownership or loses reactive tracking | High / High | Record the component/owner contract in an ADR; test scope isolation, reactive interpolation, callbacks, cleanup, and browser updates | The intended Counter and todo compile and pass browser E2E tests without manual runtime/registry plumbing |
| Native/WASM dependency leakage | Medium / High | Separate target-specific crates and features; compile each supported target in CI | Native and WASM checks run independently |
| SSR/hydration mismatch | Medium / High | Keep hydration opt-in, validate the existing DOM before enabling events, and preserve the whole SSR root as static on mismatch | Hydration mismatch and preservation tests in Phase 5.3 |
| XSS or unsafe HTML rendering | Medium / Critical | Escape by default; make trusted HTML explicit and narrowly scoped; test adversarial inputs | Security tests for text, attributes, and serialization |
| Scope growth into a full Laravel clone | High / High | Keep MVP limited to the defined vertical slice; require ADRs for major additions | Roadmap changes include rationale and acceptance criteria |
| Dependency/API churn | Medium / Medium | Pin policy rather than arbitrary versions; review dependencies and isolate integrations | Dependency review and upgrade tests |
| Weak maintenance capacity | Medium / High | Prefer small crates and a small supported surface; document maintainers and support limits | Issues and releases match available maintainer capacity |
| Performance assumptions without evidence | Medium / Medium | Establish reproducible benchmarks before optimizing or making claims | Benchmarks include methodology and baseline |
| Accessibility treated as follow-up work | Medium / High | Include semantic HTML, labels, keyboard use, and focus behavior in acceptance criteria | Accessibility checks in examples and reviews |

Likelihood and impact are qualitative planning estimates, not measured probabilities. Review this register at each phase gate.

## 9. Architecture decision records

The initial decisions and alternatives are recorded in:
- [ADR 0001 — Crate boundaries](adr/0001-crate-boundaries.md)
- [ADR 0002 — Client runtime](adr/0002-client-runtime.md)
- [ADR 0003 — Server integration](adr/0003-server-integration.md)
- [ADR 0004 — React-like component authoring](adr/0004-react-like-component-authoring.md)
- [ADR 0014 — Typed request validation and opt-in CSRF protection](adr/0014-input-validation-and-csrf-contract.md)

Each ADR's status line records whether its architectural decision is accepted or still proposed. An accepted decision explains a design choice but is not proof that every related feature is implemented; feature availability must be checked against code, tests, and the roadmap.

## 10. Phase 0.2 exit criteria

- [x] Maintainers approve target users, product principles, supported platforms, and non-goals.
- [x] MVP user stories and acceptance criteria are reviewed.
- [x] Risks have named mitigations and observable validation evidence.
- [x] The architecture decision records relevant to Phase 0.2 are reviewed and accepted or amended.
- [x] README and roadmap agree with this scope.
- [x] No planned capability is represented as implemented.

Phase 0.2 was marked complete in ROADMAP.md after its scope baseline and initial architecture/governance criteria were accepted. Keep this checklist and the roadmap synchronized if that status changes.


### CLI Fase 6.1 (alcance experimental)

El CLI de desarrollo genera un scaffold de servidor, delega `dev` y `build` a Cargo, y crea componentes/páginas/controladores/modelos/migraciones estructurales. Las dependencias Git del scaffold quedan fijadas a la revisión fuente del CLI. Los modelos no son entidades ORM. La Fase 6.2 configura perfil, host y puerto; la Fase 6.3 añade validación de entradas, CSRF optativo y redirección segura; la Fase 6.4 añade sesiones y guards experimentales. La Fase 6.5 propuesta en esta rama integra SQLx, pools SQLite/PostgreSQL, transacciones, migraciones versionadas y mapeo de errores. Esto no añade ORM ni proveedor de usuarios; las sesiones en memoria siguen siendo locales al proceso y la persistencia debe configurarse/operarse explícitamente.


## Current implementation note — Phase 6.4 (merged into main)

Phase 6.4 adds experimental server-side sessions, host-only secure cookies, pre-handler authentication guards, default-deny authorization policies and password hashing through `ring`. PR #49 is merged as `d53036410fc0d51900acf0c59b52c85f64cd1640`; the required PR checks and the post-merge CI runs passed. The included memory adapter is volatile and process-local, not production persistence. Credential verification, user-provider integration, trusted role assignment, OAuth/OIDC, MFA, account recovery and brute-force protection are not included. The current Phase 6.5 implementation proposal adds SQLx pools, explicit transactions and migrations for SQLite/PostgreSQL; the implementation must pass its CI and generated-app exit gate before the phase is closed. ORM abstractions and durable database-backed sessions remain outside this slice.
