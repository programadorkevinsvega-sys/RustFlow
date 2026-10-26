# ADR 0003: Server integration and full-stack boundary

- **Status:** Proposed
- **Date:** 2026-10-26
- **Phase:** 0.2 — Product scope

## Context

RustFlow aims to provide a Laravel-inspired full-stack workflow, but should not reinvent mature HTTP, async, database, or cryptographic infrastructure. Server functionality must remain separate from the browser runtime and must not accidentally expose server-only types or secrets to client bundles.

## Decision proposal

Use Axum, Tokio, and Tower as the initial server infrastructure candidates, behind RustFlow-owned route, request/response, error, and application-state boundaries where this adds a clear benefit. Use an incremental adapter approach: expose ergonomic RustFlow APIs while retaining the capabilities and ecosystem of the underlying infrastructure.

Use SQLx as the initial database integration candidate in a later phase. Do not introduce an ORM-like model abstraction until real CRUD use cases justify it. Use maintained libraries for cryptography and password hashing.

The MVP server scope is limited to multiple routes, safe HTML output, JSON responses, centralized error behavior, request limits/timeouts, and integration tests. Sessions, authentication, migrations, and hydration are later roadmap capabilities.

## Alternatives considered

- **Build HTTP/async infrastructure in-house:** rejected because it duplicates mature infrastructure and increases security and maintenance risk.
- **Expose Axum directly everywhere:** minimizes wrappers but can make future integration changes and RustFlow's own contracts harder.
- **Thin RustFlow-owned boundary over Axum/Tokio/Tower (proposed):** balances reuse with a cohesive API; wrappers must remain justified and not duplicate the underlying ecosystem.
- **Build a custom ORM early:** deferred because the right abstractions are unclear before application use cases exist.

## Consequences

Positive:
- Leverages mature server ecosystem components.
- Keeps client and server dependency graphs distinct.
- Allows the framework to offer conventions without reimplementing foundational infrastructure.

Costs:
- RustFlow must maintain adapters and compatibility with underlying crates.
- Abstraction layers can become needless indirection if not tied to concrete use cases.
- Dependency updates require compatibility and security review.

## Validation before acceptance

- Build a minimal route example returning HTML and JSON.
- Test unknown routes, errors, escaping, request limits, and graceful shutdown as server functionality grows.
- Confirm server-only dependencies cannot enter browser builds.
- Review the wrapper surface against direct Axum usage and simplify where a RustFlow-owned abstraction is not beneficial.
