# ADR 0035: Route-scoped rate limiting

- **Status:** Accepted for implementation
- **Phase:** 2.10 - rate limiting
- **Scope:** explicit route middleware, trusted client IP, atomic fixed-window storage

## Context

RustFlow needs a small rate-limiting contract for sensitive routes such as login,
without trusting caller-controlled forwarding headers or adding a global,
implicit scheduler. The existing HTTP boundary already resolves client addresses
according to `ClientAddressPolicy`.

## Decision

- Add an object-safe `RateLimitStore` port whose `consume` operation is atomic
  for one key and one time window.
- Build keys from the application-supplied stable scope and
  `Request::client_ip()`. The middleware never parses forwarding headers. An
  IPv4-mapped IPv6 address is normalized to its IPv4 form.
- Require the application to attach `RateLimit::middleware()` to a specific
  method/path using `Application::route_middleware`. The scope is a validated,
  constant route identifier; request paths, query values, headers, and user IDs
  cannot create unbounded bucket names.
- Use fixed windows aligned to Unix epoch seconds. Windows must be whole seconds
  from 1 through 86,400; configured limits are from 1 through 1,000,000
  requests. A configuration change for a key resets that key's current window.
- Return HTTP 429 before invoking the handler after the limit is reached.
  Responses include `Retry-After`, `RateLimit-Limit`, `RateLimit-Remaining`,
  `RateLimit-Reset`, and `Cache-Control: no-store`; the reset values are seconds
  remaining in the current window.
- Fail closed with a generic HTTP 503 when client identity, the system clock, or
  the store is unavailable. The handler is not called. No decision falls back
  to trusting an IP header or allowing the request.
- Ship `InMemoryRateLimitStore` as the first adapter. It serializes mutations
  with one mutex and is bounded to 100,000 distinct keys by default; callers may
  configure a bound up to 1,000,000. It removes expired keys when capacity is
  reached, then returns a storage-capacity error (mapped to 503) if all entries
  are still active.
- Memory counters are process-local and volatile. Restarting a process resets
  all limits; separate processes/replicas do not share counters. Applications
  requiring deployment-wide enforcement must supply a shared adapter. This
  phase does not claim distributed rate limiting.

## Alternatives considered

- Trust `X-Forwarded-For` directly: rejected. Only the existing trusted-proxy
  address policy may derive the client address.
- Derive the key from the raw URL: rejected because dynamic path values allow
  bucket cardinality attacks and can leak identifiers into storage.
- Use a token bucket or sliding window: deferred; a fixed window is the smaller
  deterministic initial contract and its boundary burst is documented.
- Add SQL storage now: deferred until its cross-engine atomicity, retention,
  and cleanup contract can be tested independently. The port allows a shared
  adapter without coupling the policy to one database.
- Evict active keys when memory is full: rejected because eviction silently
  resets a client's protection.

## Consequences and limitations

The in-memory adapter is suitable for development, tests, and explicitly
single-process deployments. It is not a distributed abuse-prevention control;
restart resets counts and a fixed window can permit up to two windows' limits
around a boundary. Per-process configuration must be combined with deployment
controls where stronger enforcement is required. Hashing and database adapters
must separately bound their own concurrency.

## Verification

Unit tests cover fixed-window boundaries, scope separation, concurrent atomic
consumption, capacity exhaustion/reclamation, invalid configuration, missing
identity, and HTTP 429/503 behavior. `scripts/rate-limit.k6.js` exercises the
running example with explicit latency, error, response-check, and rejection
thresholds.
