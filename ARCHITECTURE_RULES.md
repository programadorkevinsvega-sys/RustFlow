# RustFlow Architecture Rules

This document is normative. RustFlow uses Domain-Driven Design (DDD), Clean Architecture, Hexagonal Architecture (Ports and Adapters), and SOLID. If convenience conflicts with dependency direction, security boundaries, or testability, preserve the architecture and document an exception through an Architecture Decision Record (ADR).

## 1. Goals

RustFlow must be domain-focused, dependency-inverted, adapter-friendly, testable without infrastructure, explicit at boundaries, and secure by construction. Architecture should support a custom reactive UI runtime and a Laravel-inspired server experience without coupling core policy to vendors.

## 2. Dependency rule

Dependencies point inward:

1. **Domain/core policy:** entities, value objects, invariants, domain services, typed errors.
2. **Application:** use cases, orchestration, commands/queries, and ports.
3. **Adapters:** HTTP, DOM, database, filesystem, serialization, browser APIs, and external providers.
4. **Composition root:** CLI/server startup that wires concrete implementations.

Domain code must not import application, adapter, web-framework, database, browser, or CLI modules. Application code may depend on domain types and ports, but not concrete adapters. Adapters implement ports and translate external representations. The composition root is where concrete infrastructure is assembled.

## 3. DDD rules

### 3.1 Bounded contexts

Treat these as separate conceptual contexts until implementation proves a simpler design is better:

- **Reactivity:** signals, effects, dependency graph, scheduler, scopes, cleanup.
- **View/component model:** typed view nodes, component composition, keys, lifecycle.
- **Browser runtime:** DOM, browser scheduling, events, client cleanup.
- **Routing and HTTP:** route matching, request/response, dispatch.
- **Application runtime:** startup, configuration, dependency wiring, shutdown.
- **Rendering:** HTML generation, escaping, hydration identifiers and contracts.
- **Identity and access:** users, sessions, authentication, authorization.
- **Persistence:** repositories, transactions, migrations and database adapters.
- **Developer tooling:** CLI, code generation, build orchestration and diagnostics.

Do not create a distributed system or excessive crate count merely to reflect a diagram. Boundaries must be meaningful and testable.

### 3.2 Ubiquitous language
Use documented terms consistently: Signal, Effect, Scope/Owner, View, Component, Renderer, Hydration, Route, Handler, Middleware, Session, Guard, Repository, Transaction, Migration. Do not use one term for different concepts without qualification.

### 3.3 Domain model
- Put invariants close to the types they protect.
- Use newtypes/value objects for validated identifiers and configuration when beneficial.
- Do not leak SQL rows, HTTP requests, DOM nodes, or transport-only serialization structs into domain APIs.
- Keep UI reactivity state separate from application domain entities.
- Use domain events only for meaningful domain facts; do not introduce event sourcing by default.

## 4. Clean Architecture

### 4.1 Domain
Contains framework-owned policies and invariants. Must not depend on Axum, Tokio, Tower, SQLx, web-sys, wasm-bindgen, Clap, environment variables, or filesystem APIs. Use explicit inputs/outputs and typed errors. Avoid global mutable state and hidden service locators.

### 4.2 Application
Orchestrates use cases and coordinates domain objects. Depends on ports defined toward the inside. Does not construct concrete database, HTTP, clock, filesystem, or provider clients. Makes transactions and authorization explicit. Never expose secrets or privileged persistence details through client DTOs.

### 4.3 Adapters
Implement ports with concrete technologies. Translate and validate external data at boundaries. Map errors without losing useful diagnostic context. Do not own business policy, silently swallow failures, or turn errors into success-shaped defaults.

### 4.4 Composition root
Owns process startup, configuration, server setup, and shutdown. Wires concrete adapters to application ports. Validate production configuration before accepting traffic.

## 5. Hexagonal Architecture: ports and adapters

- Define ports for capabilities the core needs, such as repositories, transactions, session storage, clock/ID generation, or asset manifests.
- Define ports from consumer needs, not by mirroring every vendor SDK method.
- Keep ports narrow and cohesive and place them near the policy that owns their meaning.
- Put Axum, SQLx, filesystem, browser DOM, and provider implementations in adapters.
- Use in-memory adapters for deterministic core tests.
- Do not abstract a trivial single use without a meaningful boundary; document the decision.
- Avoid generic repositories that hide important database capabilities or create an untyped query language without clear need.

## 6. SOLID

### S — Single Responsibility
A module/type should have one reason to change. Separate routing, rendering, scheduling, database access, and CLI parsing. Avoid a mega-runtime that owns all concerns.

### O — Open/Closed
Extend through small traits and explicit extension points when stable use cases exist. Avoid speculative plugin systems and unnecessary dynamic dispatch.

### L — Liskov Substitution
Trait implementations must honor documented contracts for errors, ordering, cleanup, concurrency, and cancellation. Validate adapters with contract tests.

### I — Interface Segregation
Prefer narrow traits for distinct capabilities. Avoid a giant FrameworkServices interface that forces unrelated dependencies on every consumer.

### D — Dependency Inversion
High-level policy depends on stable abstractions. Inject concrete dependencies at composition boundaries; do not create them inside domain types or use cases.

## 7. Crate boundary rules

Suggested boundaries; validate them as implementation evolves:

- **rustflow-core:** foundational types and errors, without web/database/browser dependencies.
- **rustflow-reactivity:** signal/effect graph, scheduler, scopes and cleanup.
- **rustflow-vdom:** typed view representation and component contracts.
- **rustflow-macros:** procedural macros and compile-time diagnostics.
- **rustflow-dom:** browser DOM renderer and event adapters; browser-only dependencies.
- **rustflow-router:** route model/matching; separate client navigation from server dispatch where needed.
- **rustflow-server:** SSR and server runtime integration.
- **rustflow-http:** RustFlow-owned HTTP abstractions only if they add value beyond adapters.
- **rustflow-db:** SQLx integration and database adapters.
- **rustflow-cli:** commands and build orchestration.
- **rustflow:** thin public facade/prelude and feature coordination.

Rules:
- Avoid cyclic crate dependencies.
- Keep the public facade thin.
- Macros generate code; they are not a hidden runtime or business-logic layer.
- Gate optional integrations behind features where this materially reduces dependency footprint.
- Keep server dependencies out of browser builds and browser dependencies out of server-only builds.
- Centralize workspace dependency versions when practical.
- Do not create a crate only to match a diagram.

## 8. Frontend and reactivity

- Document deterministic signal/effect semantics.
- Track dependencies only in defined reactive contexts.
- Remove stale dependencies when effects rerun.
- Unmount must dispose owned effects, event listeners, and child scopes.
- Test scheduling, batching, re-entrant writes, cycles, dynamic dependencies, and cleanup.
- Avoid unsafe code unless necessary; every unsafe block needs a safety invariant, focused tests, and review.
- Do not promise Send/Sync for browser-local state unless it is actually supported.
- Keep DOM mutation in the renderer/DOM adapter; components express intent through views.
- Define keyed-list identity and hydration behavior explicitly.
- Signal reads must not trigger hidden network requests.
- Semantic HTML and accessibility are product requirements.
- **React-like authoring is a product contract, not a promise of React API compatibility.** The common component workflow must offer typed function components/props, owned ergonomic signals, automatically tracked signal interpolation, typed callback props, and deterministic cleanup.
- Do not treat the existence of `view!`, a `Dynamic` node, or a reactive full-range rerender as sufficient completion of that contract.
- Component-facing signal helpers must remain bound to an explicit runtime/owner scope. Do not use a hidden process-global mutable runtime to make `signal(value)` superficially convenient; record any alternative in an ADR and prove component/request isolation.
- Fine-grained rendering must preserve node/component identity where reconciliation says a child is the same. Tests must verify observable DOM changes and keyed identity, not just equality of generated view trees.
- Before Phase 3 closes, compile and run the intended Counter and todo examples with browser E2E tests, including callback props, conditional/keyed updates, and listener/effect cleanup.
- If the illustrative public syntax changes, update the ADR, roadmap, examples, and public documentation before implementation is accepted.

## 9. Server/client boundary

- Server-only modules must not compile into client/WASM artifacts.
- Never serialize secrets, session tokens, database credentials, or privileged objects into hydration data.
- Treat all client values as untrusted, including hidden fields and serialized state.
- Server actions must enforce server-side authentication, authorization, validation, request limits, and CSRF protections where applicable.
- Client validation is UX, not a security boundary.
- Escape text and attribute values by default.
- Raw HTML insertion must be explicit, narrowly scoped, documented, and carefully reviewed.
- Hydration mismatch recovery must not silently accept unsafe markup or duplicate event handlers.
- Map server/browser errors at boundaries while preserving safe diagnostic context.

## 10. HTTP, persistence, and security

- HTTP adapters translate protocol details; they do not own domain policy.
- Use parameterized SQL; never concatenate untrusted values into queries.
- Make transaction boundaries explicit and tested.
- Authentication and authorization are distinct; deny access by default when authorization cannot be established.
- Use maintained password-hashing and cryptographic libraries; never implement cryptographic primitives.
- Use secure session-cookie defaults: HttpOnly, appropriate SameSite, and Secure in HTTPS production.
- Protect cookie-authenticated state-changing requests against CSRF.
- Validate upload size, type, and destination; do not trust client filenames.
- Production responses must not expose stack traces, secrets, SQL, or internal filesystem paths.
- Set body limits, timeouts, and safe proxy-trust configuration.

## 11. Errors and observability

- Use typed internal errors and map them to protocol responses in adapters.
- Do not use unwrap/expect on user input, network data, environment configuration, or fallible I/O paths.
- Panics are not normal error handling.
- Logs must not contain passwords, tokens, authorization headers, or sensitive bodies.
- Use structured logs and request correlation where appropriate.
- Separate user-facing errors from internal diagnostics.
- Define cancellation and timeout behavior for async operations.

## 12. Testing

Use a test pyramid:
- Unit tests for invariants, reactivity, scheduler, routing, and serialization.
- Contract tests for ports and adapters.
- Integration tests for HTTP, databases, sessions, and SSR.
- Browser E2E tests for interactions, hydration, navigation, and cleanup.
- Fuzz/property tests for macros, parsers, routing, untrusted input, and state transitions where useful.
- Reproducible performance tests for representative workloads.

Prefer deterministic tests and in-memory adapters for core logic. Integration tests may use real services but should not make the entire suite depend on uncontrolled external networks.

## 13. Dependencies and unsafe code

- Review maintenance, license, security posture, MSRV, target compatibility, features, and compile-time impact before adding dependencies.
- Run dependency advisory checks and secret scanning in CI.
- Keep unsafe code isolated, documented, and reviewed.
- Never implement cryptographic primitives or password hashing.
- Audit CI actions and release dependencies; pin reviewed versions/SHAs where practical.
- Never commit build output, secrets, local environment files, or generated credentials.

## 14. Public API and compatibility

- Public API must be intentional and documented.
- Re-export only supported APIs from the facade/prelude.
- Use feature flags for optional integrations when they materially reduce dependency footprint.
- Avoid exposing internal types without a real contract need.
- Before 1.0, breaking changes are permitted but must be recorded with migration guidance.
- After 1.0, follow semantic versioning and a documented deprecation policy.
- Prefer explicit builder/configuration types over large positional argument lists.

## 15. ADRs and exceptions

Create an ADR for decisions affecting reactivity scheduling/lifecycle, view syntax, rendering/hydration, server/client serialization, server actions, middleware order, persistence/ORM, threading/unsafe code, major dependencies, or public compatibility. Each ADR includes context, decision, alternatives, consequences, and migration impact. Exceptions must be explicit, narrow, reviewed, and documented.

## 16. Public documentation boundary and phase gate

Public documentation is a developer-facing product surface, not a mirror of RustFlow's internal engineering system.

- Audience: application developers, framework integrators, technical decision-makers, and technology companies evaluating or using RustFlow.
- Document supported public concepts, APIs, configuration, integration contracts, user workflows, practical use cases, accurate examples, limitations, errors developers can act on, migration guidance, and relevant verification.
- Do not publish internal backend business logic, private system workflows, credentials, secrets, internal operational procedures, or implementation details that users do not need to integrate with RustFlow. Do not expose private architecture or security-sensitive internals merely to make documentation appear comprehensive.
- Public documentation may explain externally observable behavior and the security guarantees/configuration users need. Keep deeper internal design rationale, implementation notes, and ADRs in internal engineering documentation unless a deliberate public API or security explanation requires a sanitized summary.
- Never document a planned API as if it already exists. Every code sample must match the current public API, and must be labeled illustrative/proposed if it cannot be verified.
- Astro/Starlight documentation remains a static, primarily Markdown/MDX site; public docs must not depend on a dynamic backend or reveal internal backend implementation.
- At the start of every roadmap phase, review its planned work and record which public pages, examples, API references, or tutorials may need to change.
- Before declaring a phase complete or starting the next phase, review the implementation diff and explicitly confirm that required public docs were updated. If no public update is warranted, record the reason in the PR.
- Feature PRs that change a developer-visible contract must include matching public documentation and relevant examples/tests in the same PR where practical.
- Build and validate the docs site in CI whenever website content, navigation, configuration, or public documentation is changed.

## 17. Pull request checklist

- [ ] Dependency direction remains inward.
- [ ] Domain/application policy does not depend on concrete infrastructure.
- [ ] New traits represent narrow, real ports.
- [ ] Server/client boundaries remain enforced.
- [ ] Errors are explicit and do not leak sensitive details.
- [ ] Tests cover invariants, edge cases, and failure paths.
- [ ] Public API, use cases, examples, limitations, and migration notes are documented where applicable.
- [ ] Public-documentation scope was reviewed for the current phase; any no-change decision is explained.
- [ ] Public docs contain no internal backend business logic, secrets, private system workflows, or unnecessary sensitive implementation details.
- [ ] Documentation site build and navigation/link checks pass when public docs change.
- [ ] Security, accessibility, performance, and MSRV impact were considered.
- [ ] Unsafe code has a written safety argument.
- [ ] Relevant ADRs and roadmap items are updated.
- [ ] React-like authoring acceptance criteria are evaluated against compiling examples and browser behavior, not syntax similarity alone.
