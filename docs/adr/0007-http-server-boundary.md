# ADR 0007: Isolate HTTP server integration behind RustFlow-owned types

- **Status:** Proposed for review
- **Date:** 2026-10-26
- **Phase:** 4.1–4.3 — HTTP foundation

## Context

RustFlow needs HTTP routing and request/response helpers, but domain, reactivity, and structural view crates must not depend on a concrete web framework. The safe HTML renderer already exists as a separate \`rustflow-ssr\` adapter.

## Decision

Add \`rustflow-server\` as the application/server adapter. Axum, Tokio, Tower, HTTP primitives, body buffering, and request scheduling are isolated in this crate. Public handlers receive RustFlow's \`Request\`, return RustFlow's \`Response\` or \`HttpError\`, and register against a \`HttpMethod\` and route template. Application state is captured explicitly by handlers rather than resolved through a hidden service locator.

The initial adapter provides bounded body reads, a per-request timeout, a global concurrency cap, path/query/JSON accessors, standard response helpers, and bridges structural views into \`rustflow-ssr\`. Default settings cap bodies at 1 MiB, concurrent requests at 256, and request execution at 30 seconds.

## Consequences and limitations

- The core and view model remain independent of Axum/Tokio/Tower.
- Route dispatch and response helpers can be tested in memory without opening a TCP listener.
- This is not production hardening: route groups/names, middleware composition, tracing, signal-based graceful shutdown, TLS/proxy policy, rate limiting, auth/CSRF, persistence, and deployment recipes remain future work.
- \`Response::html\` is an explicit raw-string response helper and is not a sanitizer. Prefer \`html_view\` for structural content.
