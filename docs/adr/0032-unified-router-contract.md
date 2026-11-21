# ADR 0032: Unified route contract and client/server boundary

- **Status:** Accepted for review
- **Date:** 2026-11-21
- **Phase:** 2.7 - unified router
- **Scope:** HTTP routes, client navigation, SSR, route parameters, fallback,
  history, errors, and security

## Context

RustFlow currently has an application-owned HTTP route table in
`rustflow-server` and a separate opt-in browser navigation adapter in
`rustflow-dom`. The server routes execute handlers and own HTTP status,
middleware, CSRF, authorization, and fallback behavior. The browser adapter
owns same-origin link/form interception, a reactive mount, and History API
state, but it does not define a framework-owned route table.

A future router must not create two incompatible definitions of the same route
or silently treat missing client routes as successful pages.

## Decision

### Two adapters, one explicit route contract

- Keep server routing and browser navigation as separate runtime adapters.
- Introduce a shared route contract in target-independent code only after
  implementation approval; do not add hidden route state to signals or the
  component runtime.
- An application may export one declarative route manifest and construct both
  server and client routes from it. The manifest remains application-owned.
- A framework-owned global route registry or process-global dispatcher is
  rejected.
- Existing HTTP methods and Axum-style parameter syntax remain the server
  baseline unless a separately approved migration changes it.

### Route identity and parameters

- Route identity is `(HTTP method, normalized path pattern)`.
- Server and client route names must not imply that the browser can execute a
  server handler.
- Named parameters are validated as typed application inputs:
  - percent-decoding, UTF-8 validity, and malformed segments fail at extraction;
  - route values are never used directly in SQL;
  - parameter count and static/dynamic ordering must remain deterministic.
- A route manifest may support fallback routes, but a fallback is explicit.
- Query parameters are not route parameters. They use the existing
  query/form/server validation contracts and must not alter route matching.

### Matching and precedence

- Exact static routes take precedence over dynamic routes.
- Dynamic routes may coexist only with explicit and deterministic precedence.
- Duplicate normalized patterns for the same method are configuration errors.
- Ambiguous patterns, wildcard collision, and method/path collision fail closed.
- The server remains authoritative for status codes:
  - `404` for no route or an explicit server fallback;
  - `405` for an existing path with no allowed method;
  - `500` for internal server failures;
  - framework validation/action statuses remain unchanged.
- A client-only route is valid only if the application callback owns it and
  returns `Ok(true)` after updating route state. It must not claim a server
  resource exists.

### SSR and navigation

- Direct SSR requests use the server route table and must render meaningful HTML
  for the matched route.
- An explicit route fallback may provide SSR HTML, redirect, or a typed server
  error.
- The browser adapter starts only after mount or successful hydration.
- Eligible same-origin GET navigation reads the server response and commits
  route state/history only after route acceptance and a successful reactive
  view update.
- Back/forward use the existing popstate/reload fallback policy.
- Unsupported links, modified clicks, downloads, external origins, fragments,
  and alternate targets retain native browser behavior.
- A client route is not inferred from a full-document response body.

### Errors and security

- Route matching must not expose credentials, internal route tables, stack
  traces, private server state, or filesystem layout.
- Server handlers enforce authentication, authorization, CSRF, validation,
  limits, trusted-proxy policy, and data projection.
- Client route callbacks must be deny-by-default: unknown paths return
  `Ok(false)` and preserve native navigation.
- Redirect targets are same-origin and protocol-compatible; open redirect,
  loop, or scheme confusion is rejected.
- URL/query state must not carry session IDs or secrets. Browser state is
  untrusted and server authorization is always repeated.
- Rate limiting, sessions, and identity remain separate middleware/ports.

### Public API boundary

No unified router implementation is approved by this ADR. The implementation
phase may add small target-independent route descriptors and explicit
application registration helpers only after a later approval.

The implementation must:

- preserve existing `Application::get/post/put/patch/delete` APIs;
- preserve `ClientNavigation::install` behavior;
- provide typed route/parameter results;
- avoid a required async loader or data-cache protocol;
- avoid coupling browser navigation to Axum internals;
- add no implicit Suspense, hook, resource, or scheduler behavior.

## Alternatives considered

1. **Let the browser router own server routes:** rejected. It creates a
   second source of truth and cannot enforce HTTP middleware or status codes.
2. **Let Axum routes drive the browser automatically:** rejected. The browser
   needs explicit opt-in behavior, unmodified native navigation, and a
   separate mount/hydration lifecycle.
3. **Use filesystem conventions:** rejected. They hide route identity, security,
   precedence, and server-only handlers.
4. **Accept a catch-all index fallback everywhere:** rejected. SPA fallback
   must remain an explicit route policy, especially for assets and reserved
   server paths.
5. **Introduce a global route registry:** rejected. It creates cross-request
   mutation, lifecycle ambiguity, and hidden process state.

## Consequences

- Route declarations can remain application-owned while sharing a future manifest
  contract.
- Server HTTP behavior remains authoritative for direct requests and mutations.
- Browser navigation remains an opt-in enhancement rather than a replacement for
  normal links/forms.
- The first implementation will need focused route matching, fallback and
  security tests before browser E2E can claim coverage.
- This ADR does not approve a loader protocol, route data cache, nested layouts,
  streaming SSR, or server-action integration.

## Validation required before implementation

Implementation approval should include:

- unit tests for static/dynamic precedence, duplicate patterns, parameters,
  fallback, 404/405 behavior, malformed paths, and URL decoding;
- server integration tests for direct GET, SSR, middleware, 404/405, redirects,
  trusted proxy behavior, and reserved static paths;
- browser E2E tests for eligible and ineligible links, back/forward, reload,
  form fallback, stale navigation, and failed route rendering;
- documentation of migration/compatibility and the exact public API.
