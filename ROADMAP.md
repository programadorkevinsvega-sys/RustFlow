# RustFlow Roadmap

This roadmap defines a path from an empty repository to a production-grade full-stack Rust web framework. Every phase starts as planned. Mark an item complete only when its acceptance criteria are met and supported by code, tests, documentation, or release evidence. Dates are intentionally omitted.

## Vision

RustFlow aims to combine React-like reactive UI components, a Rust-owned reactivity engine, server rendering and hydration, and a Laravel-inspired full-stack developer experience: routing, handlers/controllers, middleware, validation, sessions, authentication, database integration, migrations, generators, and CLI tooling.

Reuse mature infrastructure for HTTP, async execution, database drivers, serialization, and cryptography. Do not reinvent an async runtime, database drivers, or cryptographic primitives.

## Product-level acceptance gate: React-like authoring

RustFlow is **not** intended to be a drop-in React-compatible replacement. However, its ordinary component-authoring experience must be recognizably declarative, typed, reactive, and convenient. This is a product requirement, not merely a future polish item.

The following is a **design target, not code that compiles against today's API**:

```rust
#[component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);

    view! {
        <button on:click={move |_| count.update(|n| *n += 1)}>
            "Count: " {count}
        </button>
    }
}
```

Before Phase 3 can pass its exit gate, the public API and browser runtime must demonstrate all of the following:

- An ergonomic `signal(value)` API with read, write, and update operations inside a component, while ownership remains tied to an explicit component/runtime scope. Do not introduce a process-global mutable runtime singleton as a shortcut.
- Ordinary signal interpolation in `view!` tracks dependencies and updates automatically; users do not need to wrap every normal signal child in `@dynamic`.
- A `#[component]`-style function-component API with typed props, useful compile-time diagnostics, and automatic integration into component descriptors/registration for normal use.
- Typed callback props between components, in addition to typed DOM event handlers.
- DOM updates that patch only the affected nodes/regions, preserving mounted DOM/component identity when keys and component identity indicate continuity.
- Deterministic cleanup of effects, subscriptions, event listeners, and child scopes on unmount.
- A browser-loadable example and automated browser E2E coverage for a counter and a todo list using the intended public API.

The exact public syntax and runtime ownership mechanism must be recorded in an ADR and verified with compile tests before implementation is treated as complete. Any syntax change from this illustrative target must preserve the stated developer experience or be explicitly reviewed and documented. SSR/hydration remain separate milestones and must not be implied by these client-side criteria.

## Status policy

- [ ] Planned
- [x] Complete and verified

All tasks begin unchecked. Discussion, scaffolding, or an unverified implementation does not count as completion.

## Phase 0 — Project foundation

### 0.1 Governance
- [x] Select and add a dual MIT/Apache-2.0 license.
- [x] Define contribution guidelines, code of conduct, maintainers, and decision process.
- [x] Define supported Rust toolchain and MSRV policy (stable toolchain; MSRV 1.85).
- [x] Define semantic versioning, pre-1.0 API stability, and deprecation policy.
- [x] Configure required status checks in the active `Protect main — RustFlow` ruleset. The active ruleset requires `Rust checks (1.85)`, `Rust checks (stable)`, `Build documentation`, and `Governance files and whitespace`.

### 0.2 Product scope
- [x] Define target users, product principles, supported platforms, and explicit non-goals in `docs/PRODUCT_SCOPE.md`.
- [x] Define MVP user stories and acceptance criteria in `docs/PRODUCT_SCOPE.md`.
- [x] Establish a risk register for reactivity, macros, WASM, hydration, security, and maintenance in `docs/PRODUCT_SCOPE.md`.
- [x] Create ADRs for crate boundaries, client runtime, and server integration (`docs/adr/`).

### 0.3 Engineering baseline
- [x] Establish workspace naming and dependency conventions in `docs/ENGINEERING_BASELINE.md` and the root Cargo manifest.
- [x] Verify CI for formatting, Clippy, unit tests, documentation tests, and dependency review (successful PR runs: CI, governance, and dependency review on the Phase 0.3 baseline).
- [x] Add issue/PR templates and review checklist.
- [x] Align README and contributor instructions with the initial workspace and CI baseline; re-check policy consistency during review.

**Exit gate:** project scope and policies are explicit; CI runs on pull requests; major architectural decisions have recorded context and trade-offs.

## Phase 1 — Reactivity engine prototype

### 1.1 Semantics
- [x] Specify signal reads/writes and dependency tracking (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Specify effects, nested effects, dynamic dependency replacement, and cleanup (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define scheduling order, batching, re-entrancy, and cycle behavior (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define ownership scopes and deterministic disposal (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define single-threaded browser behavior and server-side isolation/thread-safety boundaries (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).

### 1.2 Implementation
- [x] Implement typed signals (`crates/rustflow/src/reactivity.rs`).
- [x] Implement effects and dependency graph (`crates/rustflow/src/reactivity.rs`).
- [x] Implement scheduler and batching (`crates/rustflow/src/reactivity.rs`).
- [x] Implement owner scopes and disposal (`crates/rustflow/src/reactivity.rs`).
- [x] Add diagnostics for invalid/disposed runtime use (`RuntimeError` in `crates/rustflow/src/reactivity.rs`).

### 1.3 Correctness
- [x] Test that only dependent effects rerun.
- [x] Test dynamic dependencies and stale dependency removal.
- [x] Test nested effects, disposal, batching, cycles, re-entrant writes, and panic/error paths.
- [x] Add property/model-based tests where useful (dependency-free deterministic operation model; randomized property testing remains a future enhancement if justified).
- [x] Document runtime limitations and guarantees (`docs/REACTIVITY_RUNTIME.md`, `docs/REACTIVITY_VERIFICATION.md`, and the public website).

### 1.4 Baseline performance
- [x] Benchmark reads, writes, effect scheduling, and high fan-out (`crates/rustflow/benches/reactivity.rs`).
- [x] Record reproducible methodology and baseline numbers (`docs/REACTIVITY_VERIFICATION.md`; three CI repetitions with environment details).

**Exit gate:** standalone crate, documented semantics, deterministic tests, explicit cleanup behavior, and reproducible benchmarks.

## Phase 2 — View and component model

### 2.1 View representation
- [x] Define typed nodes for text, elements, fragments, and components in `crates/rustflow-view/src/lib.rs`.
- [x] Define attributes, DOM properties, boolean attributes, typed event callbacks, and deferred children.
- [x] Define key values and keyed-list identity rules: keys must be unique within a list snapshot and represent the same logical child across reorders; renderer reconciliation is implemented in Phase 3.
- [x] Define typed construction errors and the target-independent `Renderer` mount/update/unmount contract.

### 2.2 Lifecycle
- [x] Specify create, mount, update, and unmount behavior through `ComponentDefinition`, `ComponentRegistry`, and `ComponentInstance`.
- [x] Connect each component instance to an owned reactive scope; unmount disposes owned effects, child scopes, and cleanup callbacks.
- [x] Specify event-listener ownership and cleanup: platform adapters register removal callbacks with `ComponentContext::on_cleanup`; actual DOM registration/removal belongs to Phase 3.
- [x] Evaluate context/provider semantics: defer a public provider API until a concrete use case justifies it; it is not required for this initial component contract.
- [x] Document render-time side-effect constraints. Setup owns effects/resources; the repeatable render callback must produce a view and leave side effects to owned runtime scopes.

### 2.3 Declarative syntax
- [x] Prototype the dependency-free `rustflow::view!` procedural macro in `crates/rustflow-macros`.
- [x] Parse the documented constrained HTML-like grammar with Rust expressions in braces.
- [x] Support text, attributes, boolean attributes, DOM properties, events, nested elements, component descriptors, keys, and deferred children.
- [x] Emit actionable parser diagnostics using source spans.
- [x] Add compile-fail tests and document the supported syntax in rustdoc and the public component guide.

### 2.4 Tests
- [x] Test text, nested elements/components, attributes, properties, fragments, event callbacks, dynamic children, and keyed-list duplicate detection.
- [x] Test macro expansion and mismatched-closing-tag diagnostics.

**Exit gate: complete for the native structural view/component model only.** Typed view construction, reusable component definitions and registry resolution, explicit lifecycle/cleanup, and macro diagnostics are covered by tests. This does not mean the ergonomic React-like authoring contract is complete: `#[component]`, typed props/callback props, automatic reactive interpolation, browser DOM reconciliation, and E2E tests remain explicit Phase 3 requirements. It also does not close the SSR, hydration, server, or CLI roadmap phases.

## Phase 3 — Browser runtime and interactive MVP

### 3.1 React-like public authoring API
- [x] Record the public authoring contract and unresolved ownership/syntax decisions in an ADR before implementing the ergonomics layer (`docs/adr/0004-react-like-component-authoring.md`); the runtime/API implementation remains pending.
- [ ] Provide an ergonomic `signal(value)` API with documented read, write, and update operations inside components; runtime/owner-scope isolation must remain explicit and tested.
- [ ] Implement a `#[component]`-style function component API with typed props, useful missing/unknown/type-mismatch diagnostics, and automatic descriptor/registration integration for normal usage.
- [ ] Support typed callback props so parent components can pass callbacks to child components.
- [ ] Make ordinary signal interpolation in `view!` reactive by default; `@dynamic` must not be required for common signal-backed text/children.
- [ ] Add compile tests and a public example for the intended Counter API. Label the example as a design target until the code actually compiles and runs.

### 3.2 Build pipeline
- [x] Configure the wasm32-unknown-unknown target and add a dedicated CI compile check for rustflow-dom.
- [x] Integrate wasm-bindgen and web-sys in target-specific dependencies.
- [x] Separate browser dependencies from the target-independent view and reactive crates.
- [ ] Produce a browser-loadable bundle and static assets.

### 3.3 DOM runtime
- [x] Create and mount text, element, and fragment nodes through comment-anchored DOM ranges.
- [x] Apply typed attributes and properties through DOM APIs; the initial renderer uses an element allowlist and URL-scheme checks.
- [x] Attach event callbacks and remove their listeners during cleanup.
- [ ] Add targeted/dirty-subtree DOM updates. `DomRenderer::mount_reactive` now runs a render closure in an owned reactive effect and rerenders when tracked signals change, but it currently replaces the full mounted range; the target is to patch affected regions while preserving node/component identity.
- [x] Dispose registered component scopes and listeners on unmount.
- [ ] Implement conditional rendering and keyed dynamic-list reconciliation (keyed snapshots render in order, but updates currently replace the full range).

### 3.4 MVP examples
- [ ] Build a reactive counter.
- [ ] Build a todo list with add, toggle, and remove.
- [ ] Support basic form and input semantics.
- [ ] Use semantic HTML and accessible labels.
- [ ] Add browser end-to-end tests and measure bundle size.

### 3.5 Developer feedback
- [ ] Provide useful development-mode runtime errors.
- [ ] Add source maps or equivalent debugging support where feasible.
- [ ] Document clean Ubuntu setup and troubleshooting.

**Exit gate:** the Phase 3.1 authoring contract is implemented and compile-tested; the counter and todo examples use the intended ergonomic API and run in supported browsers; only affected DOM regions update where possible; keyed children preserve identity; listeners/effects clean up; CI builds browser-loadable assets and runs browser E2E tests.

## Phase 4 — HTTP server and routing

### 4.1 HTTP foundation
- [x] Integrate Axum, Tokio, and Tower behind the RustFlow-owned `rustflow-server` adapter.
- [x] Define RustFlow-owned request, response, HTTP error, method, server configuration, and application types. State is captured explicitly by route closures; a separate state container remains optional future work.
- [x] Implement route matching, path parameters, query strings, and conventional HTTP methods.
- [x] Provide text, HTML, JSON, redirect, byte-download, and status helpers.
- [x] Set request-body limits, request timeouts, and a global concurrency cap with configurable defaults.

### 4.2 Handlers and errors
- [x] Implement async route registration and reject duplicate method/path registrations.
- [x] Implement basic path-parameter and typed query/JSON decoding helpers.
- [x] Define an async closure-handler convention with explicit captured state.
- [x] Map handler errors safely and provide 404/405 routing behavior.
- [x] Add in-memory dispatch tests, unique named routes, and deterministic route introspection.
- [x] Add representative real-TCP integration tests for HTML/JSON routes, path/query decoding, 400/404/405/413/504 responses, request-ID correlation, custom error pages, body limits/timeouts, and graceful shutdown (`crates/rustflow-server/tests/http_integration.rs`).
- [ ] Add full route-group support; route groups are not part of the current server API.
- [ ] Expand network/deployment coverage for TLS/reverse-proxy trust, client aborts, malformed transport input, and concurrent/load behavior. The current local TCP suite does not cover these cases.

### 4.3 Middleware and lifecycle
- [x] Implement global ordered async middleware composition with a continuation that can wrap or short-circuit handlers. Route-scoped middleware remains future work.
- [x] Validate or generate a request ID, expose it to handlers, echo it in `x-request-id` on success and error responses, and emit structured `tracing` spans/events with method, route template, request ID, and status. Subscriber/export configuration stays application-owned.
- [x] Provide graceful-shutdown APIs driven by an injected future at the composition root. Wiring OS signals into that future remains the caller's responsibility.
- [x] Validate route templates, reject equivalent parameter-only patterns, and clamp body/timeout/concurrency configuration to safe minimums.
- [x] Document explicit closure-captured application state; production configuration and dependency-wiring validation remain pending.

### 4.4 Safe server rendering
- [x] Add rustflow-ssr to serialize supported view nodes into HTML fragments independently of an HTTP framework.
- [x] Escape text and attribute values by default; apply a scheme allowlist to URL attributes and reject multi-URL attributes until parsed.
- [x] Add safe full-document responses from structural views (`html_document`) and configurable escaped HTML error pages for selected 4xx/5xx statuses. Layout composition remains an application-level concern.
- [x] Test escaping, rejected/void elements, URL policy, component cleanup, keyed order, nesting limits, full-document charset/title/body escaping, configurable 404/405 pages, route dispatch, body limits, handler timeouts, middleware, named-route introspection, and representative TCP requests covering status/content-type/request-ID headers, error cases, body limits, timeouts, and graceful shutdown.
The broader transport/deployment tests listed under Phase 4.2 remain pending and are not implied by this rendering/test coverage.

**Exit gate:** an executable example server handles multiple routes, renders safe HTML, and returns JSON. Automated coverage includes in-memory dispatch and representative local-TCP integration tests. Wider transport/deployment coverage remains open as listed above; this prototype is not production-ready.

## Phase 5 — SSR, hydration, and full-stack data flow

### 5.1 Rendering contract
- [ ] Define server-capable and browser-only components.
- [ ] Define safe serialization for hydration data.
- [ ] Prevent server-only types and secrets entering client bundles.
- [ ] Define deterministic rendering and non-hydratable fallbacks.

### 5.2 SSR
- [x] Render structural view trees into HTML fragments using rustflow-ssr.
- [x] Expose complete HTML document responses and configurable escaped error pages through `rustflow-server` (Phase 4.4); route handlers can call these helpers.
- [ ] Add advanced layout composition and route-to-component data integration; full-stack SSR composition is not complete.
- [x] Test escaping and configured document/error-page responses.
- [ ] Test request-isolated SSR/component state and deterministic server/client boundaries.

### 5.3 Hydration
- [ ] Add minimal, safely serialized hydration data.
- [ ] Map rendered nodes to client component boundaries.
- [ ] Attach events without duplicating DOM or handlers.
- [ ] Detect and report mismatches in development.
- [ ] Define safe mismatch recovery.
- [ ] Test forms, conditions, lists, and navigation.

### 5.4 Navigation and forms
- [ ] Add client navigation after route semantics stabilize.
- [ ] Implement browser history and link interception.
- [ ] Define progressive-enhancement behavior for forms.
- [ ] Test server/client boundaries and reload fallbacks.

**Exit gate:** pages deliver meaningful HTML before client execution and hydrate correctly in supported cases.

## Phase 6 — Laravel-inspired developer experience

### 6.1 CLI
- [ ] Implement rustflow new.
- [ ] Implement rustflow dev and rustflow build.
- [ ] Generate components, pages, controllers, models, and migrations.
- [ ] Prevent generators from silently overwriting user files.
- [ ] Add deterministic CLI integration tests.

### 6.2 Configuration
- [ ] Define configuration precedence and validation.
- [ ] Add a safe .env.example without real secrets.
- [ ] Separate development, test, and production settings.
- [ ] Fail fast on invalid production configuration.
- [ ] Document secret management.

### 6.3 Validation and forms
- [ ] Implement typed input extraction and validation.
- [ ] Return structured field/form errors.
- [ ] Add CSRF protection for cookie-authenticated mutations.
- [ ] Add safe redirect-back behavior.
- [ ] Test oversized payloads and unexpected input.

### 6.4 Sessions and access control
- [ ] Define session abstraction and secure cookie defaults.
- [ ] Implement expiry, rotation, invalidation, and storage adapters.
- [ ] Implement authentication guards and identity extraction.
- [ ] Implement authorization policies with deny-by-default behavior.
- [ ] Use maintained password hashing and cryptographic libraries.
- [ ] Test fixation, CSRF, privilege escalation, and access boundaries.

### 6.5 Database and migrations
- [ ] Integrate SQLx and configurable connection pools.
- [ ] Add transaction helpers and error mapping.
- [ ] Implement versioned migrations and rollback policy.
- [ ] Design model/query abstractions only after real use cases validate them.
- [ ] Use parameterized SQL and test supported database engines.
- [ ] Document pool sizing and deployment migration practices.

**Exit gate:** a generated CRUD application validates input, persists records, uses sessions/auth, renders pages, and has repeatable integration tests.

## Phase 7 — Core framework completeness

### 7.1 Operational capabilities
- [ ] Add structured tracing and configurable log levels.
- [ ] Add safe error-reporting hooks.
- [ ] Add health/readiness endpoints.
- [ ] Define cache interfaces if justified.
- [ ] Define queue, email, and storage integrations only when delivery/security contracts are clear.
- [ ] Ensure graceful shutdown and timeouts are tested.

### 7.2 UI completeness
- [ ] Improve contexts, dynamic lists, fragments, forms, and controlled inputs.
- [ ] Define error isolation behavior.
- [ ] Add loading/async rendering semantics only when runtime design supports them.
- [ ] Document styling and asset integration.
- [ ] Include accessibility guidance and examples.

### 7.3 Testing and docs
- [ ] Provide test app builders and request helpers.
- [ ] Add isolated database and browser test utilities.
- [ ] Add a full CRUD example and an authenticated example.
- [ ] Document every stable public API and feature flag.
- [ ] Verify examples in CI where practical.

**Exit gate:** common app tasks need no undocumented workaround; APIs and examples match actual behavior.

## Phase 8 — Performance, accessibility, and DX

### 8.1 Performance
- [ ] Benchmark reactivity, DOM updates, SSR throughput, latency, and memory.
- [ ] Track client bundle size and startup cost.
- [ ] Profile large lists, nested components, and frequent updates.
- [ ] Define measurable performance budgets.
- [ ] Optimize only from reproducible measurements.

### 8.2 DX
- [ ] Improve macro diagnostics.
- [ ] Add hot reload only with documented state-preservation semantics.
- [ ] Improve incremental build behavior.
- [ ] Document editor support and formatting.
- [ ] Test dev/production behavior differences.

### 8.3 Accessibility and compatibility
- [ ] Test keyboard interaction and focus management.
- [ ] Define supported browser policy.
- [ ] Test Chromium, Firefox, and WebKit where CI permits.
- [ ] Document unsupported browser APIs and progressive enhancement.

**Exit gate:** benchmarks are reproducible, accessibility is part of review, and supported targets are explicit.

## Phase 9 — Security and supply-chain hardening

### 9.1 Threat model
- [ ] Threat-model SSR, hydration, sessions, cookies, server actions, uploads, and database access.
- [ ] Document trust boundaries and untrusted input.
- [ ] Review XSS, CSRF, SSRF, path traversal, request smuggling, and denial of service where applicable.
- [ ] Require a security checklist for sensitive features.

### 9.2 Automation
- [ ] Run dependency advisory checks and secret scanning.
- [ ] Inventory and review unsafe Rust.
- [ ] Fuzz parsers, routing, deserialization, and exposed input surfaces.
- [ ] Add regression tests for escaping and authorization.
- [ ] Audit CI actions and release dependencies.

### 9.3 Operations
- [ ] Document TLS, secure headers, cookies, proxy trust, and rate limiting.
- [ ] Ensure production errors do not leak secrets or internals.
- [ ] Document backups, restores, incident response, and key rotation where relevant.
- [ ] Verify private vulnerability reporting instructions.

**Exit gate:** release-scope threat model and security review are complete; unresolved high-severity findings block release.

## Phase 10 — Release candidate and production readiness

### 10.1 API and compatibility
- [ ] Review public APIs and experimental feature labels.
- [ ] Define 1.0 API and deprecation policy.
- [ ] Test feature combinations and supported targets.
- [ ] Audit panic paths and error propagation.

### 10.2 Reliability
- [ ] Run unit, integration, browser E2E, and migration suites.
- [ ] Run load and soak tests on representative applications.
- [ ] Test shutdown, dependency outages, timeouts, and pool exhaustion.
- [ ] Test repeated navigation and memory/lifecycle stability.
- [ ] Verify rollback and recovery procedures.

### 10.3 Deployment
- [ ] Publish production build and deployment guides.
- [ ] Document TLS/reverse proxy, environment, health checks, and asset caching.
- [ ] Document migration rollout and rollback.
- [ ] Document logs, metrics, tracing, alerting, backup, and restore.
- [ ] Produce a dependency inventory/SBOM where feasible.

### 10.4 Release
- [ ] Verify license and third-party notices.
- [ ] Publish release notes and known limitations.
- [ ] Tag a release candidate and attach reproducible test evidence.
- [ ] Obtain independent review and explicit maintainer approval.
- [ ] Publish 1.0 only after all mandatory gates pass.

**Exit gate:** security issues are resolved, compatibility is documented, deployment is tested, and maintainers approve the release.

## Phase 11 — Ecosystem and maintenance

### 11.1 Ecosystem
- [ ] Add extension points only for validated use cases.
- [ ] Publish component library and design-system guidance.
- [ ] Maintain official integrations and starter templates.
- [ ] Consider internationalization and advanced accessibility tooling.
- [ ] Establish community support pathways.

### 11.2 Maintenance
- [ ] Maintain Rust/dependency support policy.
- [ ] Publish security advisories and patch releases.
- [ ] Track browser/database compatibility and regressions.
- [ ] Maintain migration tools and upgrade guides.
- [ ] Revisit ADRs and retire obsolete abstractions.

**Exit gate:** repeatable release, support, security, and compatibility processes are established.

## MVP definition

The MVP must demonstrate:
1. A typed Rust function component with local reactive signal state and typed props, using the intended ergonomic public API.
2. Signal-backed view interpolation that updates automatically and patches the relevant DOM region.
3. Typed DOM event handlers and component callback props, with cleanup on component unmount.
4. A todo example using the same public API, with conditional rendering and keyed-list reconciliation that preserves identity.
5. A Rust server serving routes and safe HTML/JSON responses.
6. Unit, browser, and server integration tests in CI.
7. A clean-checkout developer guide.

SSR/hydration, full authentication, a custom ORM, and production deployment are later milestones. Do not imply they are part of the MVP.

## Definition of done

- [ ] Acceptance criteria pass.
- [ ] Normal, edge, and failure paths have tests.
- [ ] Public APIs and limitations are documented.
- [ ] Security, accessibility, and performance impact is considered.
- [ ] Examples and CI are updated.
- [ ] Breaking changes are recorded.
- [ ] No secrets, build artifacts, or unrelated changes are included.
