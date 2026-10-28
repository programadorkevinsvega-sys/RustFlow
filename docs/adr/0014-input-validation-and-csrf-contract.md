# ADR 0014: Typed request validation and opt-in CSRF protection

- Status: Accepted for the Phase 6.3 prototype; public API remains experimental.
- Date: 2026-10-28

## Context

The HTTP adapter can already buffer requests under a configured body limit and decode basic query/JSON values. Applications need a consistent boundary for URL-encoded forms, domain-level validation, predictable client errors, and safe post-mutation redirects. Browser cookies create a separate CSRF concern: request decoding and validation alone do not prove that a mutation was intentionally initiated by the application origin.

Phase 6.3 must not imply that sessions, identity, authorization, or persistence are already integrated. These capabilities belong to later phases.

## Decision

- Keep typed extraction and validation in the existing rustflow-server adapter. Expose Request::form, validated_form, validated_json, and validated_query, with applications implementing a small ValidateInput contract.
- Keep ValidationErrors deterministic: field names map to one or more public messages, with separate form-level messages. Decode failures return generic 400/415 errors; successfully decoded but invalid values return structured 422 JSON.
- Enforce body size before route handlers, and explicitly test oversized and unexpected form input.
- Provide opt-in CsrfProtection middleware for cookie-authenticated unsafe methods. Use maintained ring HMAC-SHA-256 and secure OS randomness for signed, expiring tokens; do not implement cryptography locally. Require an application-managed key of at least 32 bytes. The cookie and submitted token must match, and the signature must verify.
- Require application code to attach CSRF middleware to the routes/groups where browser cookies authenticate mutations. This is deliberately not a substitute for session management, authentication, authorization, or an authorization check in each protected use case.
- Provide Request::redirect_back(fallback) that accepts only a local fallback and uses a Referer path only when its origin matches the current request.
- Keep application-specific constraints, localized messages, persistence, and business decisions in application code rather than adding a generic validation framework dependency.

## Alternatives considered

- Decode forms directly into a generic map: rejected as the primary API because it leaves type and required-field guarantees implicit.
- Automatically reject every unknown field for every Rust struct: not possible generically without changing each application's deserialization policy. Applications should use #[serde(deny_unknown_fields)] when strict field sets are required; the integration test proves this behavior.
- Implement a custom HMAC or random-number generator: rejected because cryptography must use maintained libraries and operating-system randomness.
- Enable CSRF for every route automatically: rejected because the correct scope depends on which routes authenticate via browser cookies; developers must apply the middleware explicitly and still perform authorization.
- Redirect to any supplied Referer or client-provided return URL: rejected due to open-redirect risk.

## Consequences

Applications get a typed form boundary and consistent validation response without adding a new crate or framework-wide validation DSL. The API remains opt-in and experimental. Developers must manage the CSRF key safely, configure Secure cookies for HTTPS, attach the middleware deliberately, escape tokens when rendering HTML, and implement sessions/authentication/authorization separately.

## Verification gate

Before closing the phase, pass the repository's full PR checks: Rust 1.85 and stable, formatting, Clippy, workspace tests, rustdoc, WebAssembly build, browser E2E and bundle budget, website build, governance, dependency review, and hosting/deployment checks that are required by the active ruleset.
