# ADR 0007: Isolate HTTP server integration behind RustFlow-owned types

- **Status:** Proposed for review
- **Date:** 2026-10-26
- **Phase:** 4.1–4.3 — HTTP foundation

## Context

RustFlow needs HTTP routing and request/response helpers, but domain, reactivity, and structural view crates must not depend on a concrete web framework. The safe HTML renderer already exists as a separate `rustflow-ssr` adapter.

## Decision

Add `rustflow-server` as the application/server adapter. Axum, Tokio, Tower, HTTP primitives, body buffering, and request scheduling are isolated in this crate. Public handlers receive RustFlow's `Request`, return RustFlow's `Response` or `HttpError`, and register against a `HttpMethod` and route template. Application state is captured explicitly by handlers rather than resolved through a hidden service locator.

The adapter provides bounded body reads, a per-request timeout, a global concurrency cap, path/query/JSON accessors, standard response helpers, and bridges structural views into `rustflow-ssr`. It also supports global, group, and route-scoped async middleware, route groups, named routes with deterministic introspection, graceful-shutdown APIs driven by a future supplied by the composition root, rustls serving, a deny-by-default trusted-proxy policy, production HTTPS wiring checks, and validated/generated request IDs echoed in response headers. A `tracing` span records request ID, method, route template and status; subscriber/export setup stays in the application binary. Default settings cap bodies at 1 MiB, concurrent requests at 256, and request execution at 30 seconds.

## Consequences and limitations

- The core and view model remain independent of Axum/Tokio/Tower.
- Route dispatch and response helpers can be tested in memory without opening a TCP listener.
- This is not production hardening: subscriber/export setup, automatic OS-signal wiring, rate limiting, auth/CSRF, persistence, and deployment recipes remain later work. Route groups, route-scoped middleware, local TLS, and reverse-proxy trust policy are part of the Phase 4 adapter.
- `Response::html` is an explicit raw-string response helper and is not a sanitizer. Prefer `html_view` for structural content.
