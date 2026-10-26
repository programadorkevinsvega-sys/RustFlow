# RustFlow

> **Build modern full-stack web applications with Rust.**
>
> Reactive UI components, a custom reactivity engine, and a Laravel-inspired developer experience.

RustFlow is an early-stage framework project exploring a cohesive way to build web applications in Rust. The long-term goal is to combine a React-like component model and reactive client runtime with a batteries-included, Laravel-inspired full-stack workflow.

**Status: design and foundation phase.** APIs, crate boundaries, and implementation details are not stable. Do not use RustFlow for production workloads yet.

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

The workspace contains `rustflow` (single-threaded reactivity, component definitions/registry/instances, and public facade), `rustflow-view` (target-independent typed view nodes, validated names, attributes/properties/events, dynamic children, and keyed lists), and `rustflow-macros` (the constrained `view!` syntax). The native view/component contracts and macro have unit, doctest, and compile-fail coverage. An initial `rustflow-dom` WASM renderer now supports safe-subset DOM mounting, events, lifecycle cleanup, and signal-driven full-range rerenders; targeted keyed reconciliation and a browser-loadable bundle remain pending. Server routing, SSR, hydration, and CLI are still later roadmap work. These APIs are pre-1.0 prototypes and are not production-ready.

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

The workspace inherits edition, MSRV, license, repository metadata, and lint policy from the root manifest. CI is configured to test Rust 1.85 and current stable, and to run formatting, Clippy, tests, and documentation builds. Dependency review and Dependabot are configured. These workflows still need a successful GitHub Actions run to verify the baseline; browser/WASM checks and end-to-end tests will be added with the browser runtime. Do not assume checks pass until the runs confirm it.

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
