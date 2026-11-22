# ADR 0033: Durable SQL session store

- **Status:** Accepted for implementation
- **Date:** 2026-11-21
- **Phase:** 2.8 - durable sessions
- **Scope:** SQLx-backed session storage, expiry, rotation, invalidation,
  concurrency, and failure behavior

## Context

RustFlow already provides server-side opaque session IDs, secure cookie policy,
rotation, identity storage, guards, and an `InMemorySessionStore`. The current
`SessionStore` trait is synchronous, so it cannot safely own SQLx I/O from async
HTTP handlers. A database adapter must not block a Tokio worker or fall back to
silent success.

## Decision

### Async port

- Convert `SessionStore` to an object-safe async port.
- Operations are:
  `create`, `expires_at`, `get`, `set`, `rotate`, and `invalidate`.
- Use boxed futures internally so the public port remains independent of a
  specific async-trait dependency and stays usable behind `Arc<dyn SessionStore>`.
- `InMemorySessionStore` remains available and implements the same async contract.
- A store operation that loses its database connection or transaction returns
  `SessionError::StoreUnavailable`; handlers return generic `500`, never expose
  SQL text or session internals.

### SQL adapter

- Add `SqlSessionStore` over the existing `DatabasePool`.
- Support SQLite, PostgreSQL, and the MySQL/MariaDB MySQL-protocol backend.
- Use parameterized raw SQL and the existing dialect-specific pool.
- Bootstrap only the two framework-owned tables:
  `rustflow_sessions` and `rustflow_session_values`.
- Store expiry as SQLite `INTEGER` or SQL `BIGINT` (PostgreSQL/MySQL); bind the value as a checked `i64`.
- Use conflict-safe inserts so a unique-ID collision maps to `IdentifierCollision` instead of a generic storage error.
- Do not mix the session tables with user/credential tables or a user provider.
- Session values remain JSON text; sessions never store passwords or password
  hashes.

### Atomic state transitions

- Every operation checks absolute server-side expiry first.
- `create` rejects an existing ID collision and creates no active state.
- `set` updates or inserts one key inside a transaction and never extends expiry.
- `rotate` is atomic: insert the new ID, copy or clear values, then conditionally
  delete the old ID only if it is still active; the affected-row check prevents
  two concurrent rotations from consuming the same old session.
- `invalidate` removes both the session and its values atomically.
- Expired cleanup conditionally deletes only the row observed as expired, so a
  reused identifier cannot have a newly-created session removed by stale cleanup.
- Expired entries and orphan values are removed on access or by explicit
  cleanup; no operation recreates an expired or revoked session.

### Concurrency

- Unique primary keys provide final collision protection.
- A rotation loser retries a bounded number of times only for identifier
  collision.
- Storage/transaction errors do not retry as a successful operation.
- No in-process mutex is used as a substitute for database concurrency.
- SQLite remains constrained by its file/connection locking; PostgreSQL/MySQL
  use their native transaction isolation.

### Failure and security

- Session IDs are opaque and redacted in Debug output.
- SQL rows contain only opaque IDs, expiry timestamps, namespaced keys, and
  serialized public application values.
- Cookies, bearer tokens, passwords, authorization headers, credentials,
  request bodies, and SQL errors are never logged.
- Key names remain governed by the existing session key/identity restrictions.
- Storage failures fail closed for middleware and route guards.
- Dropping an operation future cancels its pending SQL work; an uncommitted transaction is rolled back when its connection is released. A transport/commit failure can have an unknown outcome and maps to `StoreUnavailable`; the adapter never retries it as success. Rotation may leave an unreachable new session until expiry if commit succeeded but the caller did not receive confirmation.

## Alternatives considered

1. **Keep `SessionStore` synchronous and block SQLx:** rejected. It can deadlock,
   block Tokio workers, or make timeout behavior nondeterministic.
2. **Use one process-local cache plus SQL writes:** rejected. It cannot prove
   durable invalidation or cross-process revocation.
3. **Mix users, password credentials, and sessions:** rejected. Identity lookup
   belongs to a separate provider port.
4. **Log SQL or session values on failure:** rejected. It can disclose cookies,
   identity, or application secrets.
5. **Use an ORM model for the session record:** rejected for this phase. The
   session adapter requires explicit, bounded SQL and dialect-aware transaction
   behavior.

## Consequences

- Existing synchronous applications must await session operations. This is an
  intentional breaking change for the experimental pre-1.0 API.
- `InMemorySessionStore` remains easy to test and does not require a database.
- SQL sessions survive process restart and can share a database across server
  processes.
- SQL session storage does not implement distributed locks, sliding expiry,
  device management, or session enumeration.
- Durable identity data still comes only from server-verified application
  authentication until Phase 2.9 adds provider ports.

## Verification

- Unit tests cover expiry, invalidation, rotation, collision, serialization,
  and store failures for memory and SQL implementations.
- SQLite integration runs real local SQL for create/get/set/rotate/invalidate.
- PostgreSQL/MySQL/MariaDB contracts run only in their existing isolated CI
  services when the environment is configured.
- HTTP integration covers logout replay, guard short-circuit, storage failure,
  cookie clearing, and concurrent rotation.
