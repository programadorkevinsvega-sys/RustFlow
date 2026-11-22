# ADR 0034: User provider and login identity ports

- **Status:** Accepted for implementation
- **Date:** 2026-11-21
- **Phase:** 2.9 - user provider
- **Scope:** Credential verification, server identity, login/logout, deny-by-default
  authorization boundaries

## Context

RustFlow has server-side sessions, `AuthIdentity`, password hashing, guards and
deny-by-default policies, but no provider port for credential lookup. The
current CRUD/demo flow hardcodes a single application credential and must not
be mistaken for a production identity system.

The provider contract must remain independent of SQL, LDAP, remote services,
OAuth, MFA, and account-recovery systems.

## Decision

- Add an async object-safe `UserProvider` port.
- Add a `Credentials` value that redacts its secret in Debug output and bounds subjects to 256 bytes and secrets to 1,024 bytes.
- Providers return `Option<AuthIdentity>` or a typed provider error.
- `StaticUserProvider` uses Argon2id hashes, verifies off Tokio worker threads, caps concurrent verifications at four per provider, and checks a dummy hash for unknown subjects; it remains a test/demo adapter, not durable identity storage.
- Providers derive roles and permissions from trusted server-side policy.
- Client JSON/forms/cookies/query parameters never assign roles or identity.
- Authentication failure and provider failure remain distinguishable internally,
  but public HTTP login responses are generic.
- Add `DenyAllUserProvider` as a safe default.
- Keep `StaticUserProvider` as a test/demo provider backed by maintained
  `PasswordHash` values. It is not a SQL user adapter or production store.
- Add `login` to verify credentials and rotate/authenticate the session, and
  `logout` to invalidate the session.
- Password hashing stays in the server session module and uses maintained
  Argon2id for new demo credentials while preserving ring PBKDF2 verification
  for legacy hashes. Provider implementations must not invent cryptographic
  primitives or move memory-hard verification onto async workers.

## Security boundaries

- Missing provider, missing user, invalid password, and provider failure do not
  authenticate.
- Provider secrets, passwords, cookies, session IDs and authorization headers
  are never included in public provider errors or Debug output.
- Login mutations require the existing CSRF protection and trusted-origin
  policies. Logout is also a mutation and must remain CSRF-protected.
- `AuthIdentity` remains server-side session state. It is not serialized from a
  client claim.
- Authorization continues to use `AccessPolicy` separately. Authentication
  does not automatically imply any permission.
- Account enumeration, reset, recovery, MFA, OAuth/OIDC, lockout, recovery
  tokens and federation require separate approved phases.

## Alternatives considered

1. **Embed SQL user lookup in the session API:** rejected. Identity storage and
   session storage are separate concerns and must remain replaceable.
2. **Accept a user object directly from request JSON:** rejected. It bypasses
   provider verification and can elevate roles.
3. **Put OAuth/MFA in this phase:** rejected. The roadmap requires separate
   approvals and lifecycle contracts.
4. **Log provider and password details:** rejected. It leaks credentials or
   enables account-enumeration diagnostics.
5. **Default allow on missing provider:** rejected. The provider boundary is
   deny-by-default.

## Consequences

- Applications integrate credential lookup through `UserProvider` without
  coupling server routing to a database crate.
- The static provider is useful for tests and demos but is not a user database.
- Login/logout remain application HTTP flows plus server session invalidation.
- A future SQL/LDAP/remote provider can implement the async port without changing
  guards or authorization policy.
- No production identity lifecycle guarantee is implied.
