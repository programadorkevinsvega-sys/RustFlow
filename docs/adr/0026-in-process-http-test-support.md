# ADR 0026: In-process HTTP test support

- **Status:** Accepted
- **Date:** 2026-11-01
- **Decision owners:** RustFlow maintainers

## Context

The server integration suites exercise the actual HTTP adapter, but several tests
repeat helpers that bind ephemeral TCP listeners, spawn a server task, send raw HTTP
bytes and coordinate shutdown. That coverage is necessary for transport contracts,
but it makes ordinary handler, middleware, request parsing and response assertions
more expensive and harder for application developers to reuse.

Application owns the route registry and assembles a private Axum router. Creating a
second router or manually invoking route handlers would fail to test the framework's
real pipeline. A separate testing crate would need a new public router-conversion
boundary or would duplicate adapter internals.

## Decision

Add rustflow_server::testing behind the opt-in testing Cargo feature. The module
builds the same router as Application::serve and dispatches HTTP requests in-process
through Tower's ServiceExt::oneshot. It provides test application configuration,
request builders, bounded response collection, and typed JSON helpers.

The utilities remain in the server adapter rather than a new workspace crate because
they require access to the adapter's private router assembly and existing request
pipeline. The feature is disabled by default, introduces no new dependency, and can
be enabled only for tests through a dev-dependency.

## Alternatives considered

1. **Dedicated rustflow-test crate:** rejected for the initial API because it would
   require exposing router internals across crates or duplicating the server adapter.
2. **Direct handler invocation:** rejected because it bypasses route matching,
   middleware, request metadata, parsing, request limits and configured error pages.
3. **TCP for every test:** rejected as the default because it adds socket/process
   lifecycle work to tests that do not assert transport behavior. TCP remains required
   for connection, TLS, peer-address and shutdown tests.

## Consequences

- Application tests and external application developers share the same opt-in helper.
- A test dispatches through the real route/middleware pipeline and does not start a
  listener or reserve a port.
- The feature is not part of the default server build.
- In-process requests have no peer socket address and cannot verify TLS, trusted proxy
  policy, connection lifecycle, client disconnects, graceful shutdown or cancellation.
  Those contracts continue to require real transport integration tests.
- SQL/ODM and browser isolation are separate Phase 7.3 contracts and are not provided
  by this decision.
