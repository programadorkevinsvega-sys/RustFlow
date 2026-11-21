# ADR 0029: Typed JSON server actions

- Status: Accepted for the initial experimental contract
- Date: 2026-11-21

## Context

RustFlow already has an HTTP request pipeline with typed JSON extraction, server-side validation, request-size limits, timeouts, ordered middleware, CSRF, session identity, and default-deny permission guards. It does not have a server-action API. An action must compose those existing controls rather than create a second dispatcher, trust client validation, or accept serialized client identity.

The initial contract must also define request/response serialization, cancellation, and finite data limits. Progressive form submissions, client proxies, and a general remote-procedure-call system would add separate browser and navigation semantics.

## Decision

### Public API and invocation

- Add `Application::action` and `RouteGroup::action` as explicit HTTP `POST` routes.
- The wire format is JSON only. The request must use `Content-Type: application/json` (parameters such as `charset=utf-8` are allowed). URL-encoded forms and progressive HTML redirects are not actions in this version.
- The request type must implement `DeserializeOwned + ValidateInput`. RustFlow deserializes and runs `ValidateInput` on the server before invoking the callback. Applications should use `#[serde(deny_unknown_fields)]` where unknown input members must be rejected.
- The callback returns a serializable output or `HttpError`. Success is HTTP 200 with the value encoded directly as JSON, `Content-Type: application/json; charset=utf-8`, `Cache-Control: no-store`, and `X-Content-Type-Options: nosniff`.
- No client-side validation, hidden field, serialized state, role, or identity is an authorization input. Application business rules and persistence remain in the callback/application layer.

### Mandatory security declarations

- `ActionConfig::new(access, csrf)` requires both declarations as arguments and has no `Default` implementation. Omitting either at a call site is a compile error.
- `ActionAccess` is explicitly `Public`, `Authenticated`, or `Permission { permission, policy }`. Permission policy errors and missing grants fail closed. Session-based modes require the application to install `SessionManager::middleware()` before requests are served.
- `ActionCsrf` is explicitly `NotRequired` or `Required(CsrfProtection)`. Session-authenticated and permission-guarded actions cannot be configured without `Required`; construction fails. Public actions must explicitly choose whether CSRF is needed. Use `Required` whenever browser cookies influence authorization or mutation behavior.
- `CsrfProtection` retains its existing signed-token, origin-check, and unsafe-method behavior. A CSRF token is not authentication or authorization.
- The first action API supports public and session-cookie access only. Applications with bearer-token or custom authentication continue to use ordinary routes and their own explicit middleware; this ADR does not infer a security scheme.

### Request and response limits

- The existing global `ServerConfig::with_max_body_bytes` limit is enforced while buffering the request, before action dispatch.
- Per-action request limit defaults to 64 KiB, may be configured up to 1 MiB, and must be less than or equal to the global body limit. Registration fails with `ServerError::ActionRequestLimitExceedsGlobal` if it is greater; the action limit never raises the global limit. A deployment with a global limit below 64 KiB must configure each action to a limit no greater than that global value.
- The per-action limit middleware runs before that action's access/CSRF checks and callback. Global and group middleware still run earlier, but the body has already been bounded by the global limit.
- The serialized JSON response defaults to 256 KiB and may be configured up to 4 MiB. RustFlow serializes through a bounded writer that never appends bytes beyond the configured limit.
- If serialization fails or would exceed the output limit, RustFlow discards the buffered partial serialization and returns the configured generic HTTP 500 response. No partial JSON body is sent. The action callback has already run, so RustFlow cannot undo side effects; use explicit transaction boundaries and do not treat a 500 as proof that no mutation occurred.
- Uploads, streaming action responses, custom status/headers, and larger-than-limit payloads are outside this contract.

### Errors

- Unsupported/missing media type: 415.
- Malformed JSON or type/shape mismatch: 400 with a generic parser message.
- Successfully decoded input that fails `ValidateInput`: 422 with the existing deterministic `ValidationErrors` JSON shape.
- Global or per-action request body limit exceeded: 413; the action callback is not invoked.
- Missing session identity: 401. Permission denial or CSRF/origin failure: 403. Policy/storage/internal errors use a generic 500.
- Application callback errors use `HttpError`; its public message is the application's responsibility and must not contain secrets or internal diagnostics.
- Request timeout: 504. Other HTTP error-page configuration continues to apply. Responses and tracing use the existing request ID pipeline; request bodies, CSRF tokens, and session identifiers are not logged by the adapter.

### Cancellation and side effects

- Actions use the existing request timeout (30 seconds by default); it bounds body reading and the asynchronous request pipeline. A timeout drops the active action future and returns 504. Synchronous JSON parsing/serialization and non-yielding callback code cannot be preempted in the middle of a poll; finite body/output limits bound JSON work but are not a CPU deadline.
- Graceful server shutdown stops accepting work, drains active requests, then cancels remaining request pipelines when its configured shutdown deadline expires. The server reports `ServerError::ShutdownTimedOut` if the deadline cannot drain all in-flight work.
- Cancellation is dropping a Rust future, not rollback or compensation. Async operations should be cancellation-safe and bounded; database changes should use explicit transactions. Non-yielding synchronous code and external side effects cannot be forcibly or reliably undone. The initial API adds no action-specific cancellation token, retry, or idempotency guarantee.

## Alternatives considered

- **Accept JSON and URL-encoded forms immediately:** deferred to avoid mixing JSON API response semantics with browser redirect/form errors. Existing ordinary routes and native forms remain available.
- **Implicit public access or optional CSRF defaults:** rejected because omitted security declarations are easy to mistake for protected behavior. Both access and CSRF must be passed to the action configuration constructor.
- **Reuse only arbitrary route middleware:** rejected for the first action API because the access intent and cookie-CSRF decision would not be visible in the action registration contract.
- **Client-serialized identity or authorization state:** rejected; only server-side session identity and policies are authoritative.
- **Unbounded output buffering or truncating JSON:** rejected because either can consume unbounded memory or produce success-shaped invalid JSON.

## Consequences

- The feature is an experimental, typed JSON POST abstraction over the existing route pipeline, not a client-side `use server` transform or full RPC framework.
- Existing middleware, validation, sessions, guards, request correlation, limits, timeout, cancellation, and configured error pages remain authoritative.
- User code remains responsible for authorization decisions not represented by the built-in session/permission modes, persistence, transaction safety, idempotency, and production rate limiting.
- The public guide documents current usage and limits. It does not promise progressive forms, automatic client proxies, or production readiness.

## Documentation impact

- Add the public Server Actions guide and sidebar entry.
- Update the public capability table, server/API reference, README summary, and Phase 2.4 status.
- Keep authorization implementation details, response failure semantics, and cancellation rationale in this ADR; publish only the supported API and externally observable guarantees.

## Verification gate

- Unit and compile-fail tests for mandatory security declarations and bounded configuration.
- HTTP tests for success, 400, 413, 415, 422, CSRF, unauthenticated/unauthorized, callback/serialization errors, output limits, and timeout cancellation.
- Run the repository k6 scenario against a real local TCP server, record the exact command and observed thresholds, and avoid treating the result as a production capacity guarantee.
- Pass the final PR's required Rust, documentation, WASM/browser, governance, and dependency checks before closing Phase 2.4.
