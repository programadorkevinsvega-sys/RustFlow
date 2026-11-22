# ADR 0015: Server-side sessions and access control

- Status: Accepted for Phase 6.4; implementation merged in PR #49. Public APIs remain experimental.
- Date: 2026-10-29

## Context

RustFlow already owns HTTP requests, responses, ordered middleware, route-scoped middleware, trusted HTTPS detection, and an opt-in CSRF middleware based on ring. Phase 6.4 needs server-side sessions, authentication identity, pre-handler guards, default-deny authorization, and password hashing. Database persistence remains Phase 6.5.

## Decision

- Keep sessions in rustflow-server. Do not introduce a second HTTP stack or cookie system.
- Store session data and identity on the server; cookies carry only an opaque 256-bit OS-random identifier.
- Define SessionStore operations that must be atomic per session. Rotation installs a fresh ID and invalidates the old ID atomically. Writes never recreate expired/revoked records. InMemorySessionStore uses one lock and is explicitly volatile and process-local.
- Expire sessions according to absolute server-side timestamps. Cookie Max-Age is derived from the same expiry.
- Default to the host-only __Host-rustflow_session cookie with Secure, HttpOnly, SameSite=Lax, Path=/ and no Domain. Plain HTTP requires the explicit development policy with a non-prefixed cookie name.
- Rotate and clear pre-authentication values when authenticate is called. The caller must first verify credentials. Explicit rotate preserves app values; logout/invalidate revokes the old ID.
- Identity, roles and permissions are stored only in the server-side session. Client-supplied identity claims are not read. route_protected and route_authorized attach the guard during route registration so it runs before the handler.
- Missing policies/rules and policy errors deny by default. Authentication failure is 401, authorization denial 403, and storage/policy errors produce a generic 500.
- Retain ring's PBKDF2-HMAC-SHA-256 format (600,000 iterations, random 16-byte salt, 32-byte derived key) for existing credentials. Phase 2.12 adds a maintained Argon2id option for new hashes and an explicit legacy rehash signal; it does not migrate stored credentials. See ADR 0037 for the fixed Argon2id profile and concurrency limits.
- CSRF remains a separate stateless signed-token mechanism. Session middleware does not disable CsrfProtection. Attach it to cookie-authenticated unsafe routes, including login/logout; issue fresh CSRF state when an authentication transition needs to renew browser form state.
- No database adapter is added in Phase 6.4. A durable adapter must preserve the documented atomicity, expiry and revocation contract.

## Threat model and trust boundaries

Request headers, cookies, JSON/forms, query parameters and path parameters are untrusted. The session ID is only a lookup key, not an identity assertion. Application code that verifies credentials and calls SessionContext::authenticate is trusted. Role assignment must derive from trusted server-side user or policy data.

SessionStore implementations are trusted infrastructure. They must check expiry during access, never recreate revoked sessions through set, implement rotation and invalidation atomically, and fail closed on storage errors. Secure cookies assume Request::is_https is trustworthy because TLS is terminated in RustFlow or forwarded scheme data comes only from configured trusted proxies. The rate-limit contract in ADR 0035 similarly trusts only the resolved Request::client_ip, not raw forwarded headers.

CSRF tokens are not cryptographically bound to a session ID. OAuth/OIDC, MFA, account recovery, distributed revocation, and production deployment hardening are not implemented in this phase. UserProvider, durable SQL sessions, route-scoped process-local rate limiting, and Argon2id are defined in later Phase 2 subphases.

## Alternatives considered

- Client-side identity claims in cookies: rejected because clients can modify them.
- A second HTTP/session layer: rejected because the request/response/middleware APIs already exist.
- Default allow when a policy is missing: rejected; lack of policy must not grant access.
- Database sessions now: initially rejected for Phase 6.4 because SQLx, migrations and transaction strategy were later scoped separately. Phase 2.8 now adds `SqlSessionStore` over `DatabasePool` with explicit async transaction behavior.
- Custom cryptographic code: rejected. ring provides maintained random generation, HMAC for CSRF and legacy PBKDF2; the maintained RustCrypto Argon2 crate provides the Argon2id option.
- CSRF bypass for login/logout: rejected because browser-cookie transitions remain CSRF-relevant mutations.

## Consequences and limitations

The API is experimental and pre-1.0. In-memory sessions are lost at restart and cannot be shared between instances. The SessionStore port is async; SqlSessionStore supplies the durable SQL adapter. PBKDF2 and Argon2id are CPU-intensive; route-scoped rate limiting is available separately but its in-memory adapter is not distributed. Guards are applied by route_protected/route_authorized or by explicitly attaching middleware to routes. No broad security audit is implied.

## Verification gate

Unit tests cover expiry, invalidation, rotation, password verification and guard short-circuiting. TCP integration tests cover anonymous rejection, login rotation, CSRF-protected login/logout, role denial and replay after logout. These cases are not proof of overall security. Required Rust, WASM, browser, docs, governance and dependency checks must pass on the final PR HEAD before the roadmap phase can be closed.
