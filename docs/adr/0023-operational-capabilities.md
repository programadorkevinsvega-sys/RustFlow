# ADR 0023: Operational capabilities and lifecycle contracts

- Status: Proposed for review with Phase 7.1 implementation
- Date: 2026-10-31
- Scope: tracing configuration, safe error reporting, health/readiness, graceful shutdown deadlines

## Context

The HTTP adapter already emits structured tracing spans with request ID, method, route template and status events. It also exposes request timeouts, a caller-owned shutdown future, a 30-second request timeout, bounded concurrency, TLS handshake limits and independent SQL/MongoDB timeouts. Phase 7.1 must extend these foundations without introducing another HTTP runtime, replacing the existing request pipeline, or taking ownership of process-wide observability setup.

The framework needs operator-facing readiness semantics and internal error notification, but must not disclose request secrets, dependency diagnostics or internal stack information to clients. Applications also need a finite upper bound for graceful draining when requests remain active.

## Decision

### Tracing and log levels

- Keep `tracing` as the event/span API already used by the server.
- Add `TracingConfig` and `LogLevel` to resolve `RUSTFLOW_LOG_LEVEL` via `AppConfig`.
- Support `error`, `warn`, `info`, `debug`, and `trace`; default to `info`.
- Record request duration in the existing `http.request` span and completion event.
- Do not initialize a global subscriber from a library crate. The application binary owns subscriber initialization, filters, formatting and exporters.
- Keep HTTP log fields allowlisted: request ID, method, registered route template, status and elapsed duration. Do not log headers, cookies, authorization data, query values, or request bodies.

### Error reporting

- Provide an application-owned `ErrorReporter` callback receiving `ErrorReport`.
- The public report is restricted to request ID, HTTP method, registered route template, HTTP status and a safe category (`handler`, `middleware`, or `request_timeout`). It intentionally does not carry an error message, source chain, stack trace or request data.
- Invoke the reporter for server-error (5xx) outcomes, including handler failures and request timeouts; expected 4xx responses are not reported.
- Run reporting in an isolated Tokio task and bound the wait with an application-configurable timeout (250 ms by default). Abort a timed-out task.
- A reporter error or panic is logged with a generic message and cannot replace the HTTP response or recursively invoke the reporter.
- The reporter is an integration boundary, not a vendor-specific SDK. Implementations must not block executor threads or spawn unbounded detached work.

### Health and readiness

- Provide opt-in framework endpoints to avoid breaking applications that already own common health paths. Defaults are `/health/live` and `/health/ready`.
- Liveness is lightweight and independent of external dependencies.
- Readiness executes only explicitly registered required checks, concurrently, with a timeout per check. At most 32 checks may be registered.
- A successful set of checks returns 200; any failed, panicking or timed-out check returns 503.
- Public bodies are fixed generic JSON values and include no check names, provider errors, credentials or infrastructure details. Responses use `Cache-Control: no-store`.
- Health paths are static absolute paths and cannot overlap an exact application route.

### Graceful shutdown

- Preserve the caller-provided shutdown future and existing HTTP/TLS listener APIs.
- Add a configurable shutdown drain deadline (30 seconds by default).
- Once the future resolves, stop accepting new connections and allow in-flight work to drain until the deadline.
- On deadline expiry, signal request-level cancellation, drop active handler futures (including requests waiting for a concurrency permit), wait for tracked request guards to drain, then drop the server future and return `ServerError::ShutdownTimedOut`.
- The application owns signal wiring. The framework does not install process-global signal handlers.
- This deadline is a cancellation boundary, not a guarantee that arbitrary external side effects are reversible. Applications remain responsible for transaction-safe cancellation and resource cleanup.

### Deliberately deferred integrations

- Cache interface: deferred. No framework use case currently justifies a shared/local abstraction with key namespaces, TTL, invalidation, concurrency and consistency semantics.
- Queue: deferred until delivery, acknowledgements, retry/backoff, dead-letter, durability, idempotency and worker-shutdown guarantees are specified.
- Email: deferred until provider acceptance versus actual delivery, credential handling, retries, idempotency, content and header injection protections are defined.
- Storage: deferred until object identity/isolation, access controls, URL signing, size/type validation, consistency and deletion semantics are defined.

No placeholder traits are introduced for these deferred integrations.

## Alternatives considered

1. Install a global tracing subscriber from `rustflow-server`: rejected because a library must not take ownership of process-wide subscriber initialization or conflict with the application's exporter/layer stack.
2. Add a mandatory observability vendor SDK: rejected because it couples the framework to a provider and may impose credentials, network behavior and dependency cost on all applications.
3. Expose raw HTTP errors and request data to reporters: rejected because it creates a secret-leak path and couples integrations to internal error representation.
4. Register all health endpoints by default: rejected because applications may already own these paths and implicit route registration is a breaking change.
5. Probe all configured databases automatically: rejected because not every registered database is required for every route/instance; readiness is explicit opt-in.
6. Add a generic cache/queue/email/storage trait now: rejected because an interface without concrete delivery/security semantics would be a false capability.
7. Keep graceful shutdown unbounded: rejected because a stuck connection could prevent process termination indefinitely.

## Consequences

- Existing request tracing and route registration remain the foundation.
- Applications gain an explicit log-level configuration input but retain subscriber ownership.
- Error reporting is provider-neutral and bounded, with a deliberately small metadata contract.
- Health/readiness are opt-in and do not infer dependency requirements.
- Existing `serve_with_shutdown`, `serve_listener_with_shutdown`, and TLS shutdown entry points retain their signatures; the new default deadline means their shutdown can now return `ShutdownTimedOut` instead of waiting forever.
- Cache, queue, email and storage remain unavailable and must not be documented as implemented.

## Security and operational risks

- Request IDs are for correlation only and never authorization.
- Route templates may contain parameter names but not parameter values; health responses never enumerate dependencies.
- Reporters may still persist data they receive, so the allowlisted metadata contract is intentionally small.
- Reporter callbacks must avoid synchronous blocking; timeout/abort protects the request path but cannot reverse external side effects already completed.
- Dropping a stuck handler can cancel application code. Database transactions must use the existing rollback/commit guarantees and must not assume cancellation means a remote side effect did not happen.
- Readiness checks must be idempotent and read-only; do not perform migrations or mutate application data from probes.

## Verification required for acceptance

- Unit tests for log-level parsing, path validation, report metadata allowlisting and readiness check construction.
- HTTP tests for liveness, successful/failed/timed-out readiness, generic responses and no information disclosure.
- Reporter tests for allowlisted context, errors, panics, timeout and unchanged public 500 behavior.
- Real TCP test proving the shutdown deadline expires for a stuck active request and drops its handler future.
- Rust 1.85 and stable formatting, Clippy, workspace tests and documentation build, plus all repository-required WASM, browser, governance, dependency review and site checks on the final PR SHA.
