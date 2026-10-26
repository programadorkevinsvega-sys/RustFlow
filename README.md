# RustFlow

> **Build modern full-stack web applications with Rust.**
>
> Reactive UI components, a custom reactivity engine, and a Laravel-inspired developer experience.

RustFlow is an early-stage framework project exploring a cohesive way to build web applications in Rust. The product target is a React-like component-authoring experience—typed function components and props, owned reactive signals, automatic reactive view interpolation, typed callbacks, and efficient DOM updates—combined with a batteries-included, Laravel-inspired full-stack workflow. This is the target contract, not a claim that those APIs are all available today; see the roadmap and [ADR 0004](docs/adr/0004-react-like-component-authoring.md).

**Status: early pre-1.0 prototype.** The repository includes an initial reactive runtime, typed view/component model, a preliminary WASM DOM renderer, and a prototype HTTP server with routing, JSON, safe HTML documents, and configurable error pages. APIs remain unstable; RustFlow is not production-ready.

## Project goals

- **Reactive UI:** components, signals, effects, event handling, conditional rendering, and dynamic lists.
- **Own reactivity engine:** implement and test RustFlow's reactive graph and scheduling model rather than wrapping another UI framework.
- **Full-stack Rust:** server-side routing, handlers/controllers, middleware, validation, sessions, database integration, and server-rendered HTML.
- **Laravel-inspired ergonomics:** conventions, generators, migrations, configuration, and a helpful CLI.
- **Rust-first engineering:** explicit types, predictable errors, memory safety, clear module boundaries, and strong tests.
- **Production readiness:** security reviews, observability, compatibility policy, performance budgets, and documented operational practices.

## Non-goals for the first MVP

- Reimplementing an entire ORM, database driver, HTTP server, cryptographic library, or async runtime.
- Supporting every browser API or every HTML feature on day one.
- Promising API stability before the framework has real usage and compatibility tests.
- Claiming production readiness before the release gates in [ROADMAP.md](ROADMAP.md) are met.

## Planned architecture

The workspace contains `rustflow` (single-threaded reactivity and component lifecycle), `rustflow-view` (target-independent typed nodes), `rustflow-macros` (the constrained `view!` syntax), `rustflow-dom` (an initial safe-subset WASM DOM renderer whose reactive updates replace the mounted range), `rustflow-ssr` (escaped HTML-fragment serialization), and `rustflow-server` (a bounded Axum/Tokio/Tower HTTP adapter with RustFlow-owned request/response types, global ordered middleware, named-route introspection, injectable graceful shutdown, complete safe HTML documents, and configurable 4xx/5xx HTML error pages). The server example covers HTML, JSON, query/path parameters, JSON input, middleware, named-route introspection, and Ctrl+C shutdown wiring. The adapter propagates validated/generated request IDs and emits `tracing` spans/events; subscriber/export configuration remains application-owned. Representative real-TCP integration tests cover routes and responses, custom error pages, request-ID headers, body limits, timeouts, and graceful shutdown. Targeted/keyed DOM updates, conditional rendering, browser bundles/E2E, route-scoped middleware, route groups, broader transport/deployment testing, OS-signal wiring inside the framework, hydration, and CLI scaffolding remain pending. These APIs are pre-1.0 prototypes and are not production-ready.

| Area | Planned responsibility |
| --- | --- |
| Public facade | Ergonomic prelude and feature flags |
| Reactivity | Signals, effects, dependency tracking, scheduling, ownership and cleanup |
| View layer | Typed view nodes and component composition |
| Macros | Compile-time transformation of the declarative view syntax |
| Browser runtime | DOM creation/patching, event delegation and client lifecycle |
| Router | Route matching, parameters and navigation |
| Server runtime | HTTP integration, handlers, SSR and response types |
| CLI | Project scaffolding, dev server, build and code generators |
| Integrations | Validation, sessions, auth, SQLx-backed data access and migrations |

See [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md) for mandatory design constraints and [ROADMAP.md](ROADMAP.md) for phases, deliverables and exit criteria.

## Proposed technology choices

These are starting points to validate, not irrevocable commitments:

- Rust stable and Cargo workspaces
- Axum, Tokio and Tower for server infrastructure
- SQLx for initial database integration
- `syn` and `quote` for procedural macros
- `wasm32-unknown-unknown`, `wasm-bindgen` and `web-sys` for the browser target
- Serde for data serialization
- Clap for the CLI

Core UI reactivity and component lifecycle behavior are RustFlow-owned. Infrastructure should be reused when it is mature and does not undermine the project's goals.

## Development principles

1. Build the smallest vertical slice that proves the architecture.
2. Write tests alongside implementation; avoid untested abstractions.
3. Keep server-only code and secrets out of client/WASM builds.
4. Make unsafe Rust exceptional, isolated, justified, and reviewed.
5. Prefer stable, documented APIs over clever abstractions.
6. Document design decisions and migration paths before breaking public APIs.
7. Treat accessibility, security, performance, and developer experience as product requirements.

## Getting started

The workspace contains the single-threaded reactive runtime and the initial structural view model. Neither is a complete application framework. From the repository root, run the baseline checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The workspace inherits edition, MSRV, license, repository metadata, and lint policy from the root manifest. CI is configured for Rust 1.85 and current stable, formatting, Clippy, workspace tests, Rust documentation, a `wasm32-unknown-unknown` compile check, governance, and dependency review. The server includes representative real-TCP integration tests. A browser-loadable bundle and browser end-to-end tests remain pending; check the latest GitHub Actions runs rather than assuming any new commit has passed.

## Contributing and governance

Contributions are welcome under the project's dual MIT/Apache-2.0 license. Before contributing, read:

- [CONTRIBUTING.md](CONTRIBUTING.md) for workflow and quality expectations.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community standards.
- [MAINTAINERS.md](MAINTAINERS.md) for maintainership and decision-making.
- [AGENTS.md](AGENTS.md) for repository-wide instructions for contributors and coding agents.
- [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md) for DDD, Clean Architecture, Hexagonal Architecture and SOLID rules.
- [docs/ENGINEERING_BASELINE.md](docs/ENGINEERING_BASELINE.md) for workspace and dependency conventions.
- [ROADMAP.md](ROADMAP.md) for project scope and acceptance criteria.
- [SECURITY.md](SECURITY.md) for responsible vulnerability reporting.

When proposing a major dependency, crate boundary, public API or architectural change, include the problem, alternatives considered, trade-offs, and migration impact in the pull request. Repository branch protection and required status checks are configured separately in GitHub settings.

## License

RustFlow is dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

You may choose either license when using, modifying, or distributing the project, subject to its terms.
