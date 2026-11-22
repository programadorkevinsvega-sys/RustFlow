# ADR 0037: Argon2id password hashing option

- **Status:** Accepted for implementation
- **Phase:** 2.12 - Argon2id password hashing
- **Date:** 2026-11-21
- **Related:** ADR 0015, ADR 0034

## Context

`PasswordHash` currently creates and verifies a RustFlow-specific PBKDF2-HMAC-
SHA-256 representation. New credentials should have a memory-hard option without
invalidating existing hashes or silently changing how persisted records are
interpreted.

## Dependency evaluation

- Use RustCrypto's maintained `argon2` crate, pinned to 0.6.0.
- The crate declares `MIT OR Apache-2.0`, Rust 1.85 MSRV, and implements Argon2d,
  Argon2i, and Argon2id. RustFlow's declared workspace MSRV is Rust 1.94.
- Its `password-hash`/PHC API supplies encoding and verification; RustFlow does
  not implement the Argon2 primitive or PHC encoding.
- The dependency audit is a final verification gate for the exact workspace
  lockfile; no advisory was identified in the crate metadata review.

## Decision

- Add `PasswordHash::hash_argon2id` using Argon2id v=19 and a fixed profile:
  19,456 KiB memory, two iterations, one lane, 16-byte random salt, and 32-byte
  output. The encoded string is PHC form with those exact versioned parameters.
- Keep `PasswordHash::hash` as an explicit legacy PBKDF2-HMAC-SHA-256 generator
  (600,000 iterations, 16-byte random salt, 32-byte output) for compatibility.
  `parse` and `verify` accept only the fixed legacy encoding or the fixed
  Argon2id profile. PHC work factors are checked before running the KDF, so an
  untrusted stored string cannot request arbitrary memory or CPU work.
- Limit passwords to 1,024 bytes and encoded hash strings to 512 bytes.
- Expose `needs_rehash()` as a signal for the legacy PBKDF2 format. It does not
  update a user record. Applications may migrate only after successful
  verification and a successful explicit credential-store update; verification
  never silently rewrites credentials.
- Keep `PasswordHash::verify` synchronous and document that it is blocking.
  `StaticUserProvider` uses Argon2id for demo hashes and dummy unknown-user
  checks, executes verification with `spawn_blocking`, and allows at most four
  concurrent verifications per provider. Saturation returns a generic provider
  error rather than queuing an unbounded number of blocking KDF jobs.
- Do not add a pepper, a global singleton, automatic database migration, or a
  production user database in this subphase.

## Consequences and limitations

Argon2id consumes about 19 MiB per verification; the `StaticUserProvider` cap
can therefore use about 76 MiB per process, in addition to runtime overhead.
The cap applies to this demo provider only; custom providers must bound their
own CPU/memory work. Hash creation is synchronous and belongs at startup or on a
blocking worker. No hardware-specific latency benchmark or production
credential migration is claimed.

Legacy hashes continue to verify, but `needs_rehash()` is advisory and no user
records are migrated automatically. Applications must implement explicit
transactional persistence and recovery behavior around an upgrade.

## Alternatives considered

- Replace PBKDF2 immediately: rejected because it would strand existing
  credentials and silently change persisted-format expectations.
- Verify PHC parameters from the stored string without fixed bounds: rejected
  because attacker-controlled cost parameters can cause excessive memory or
  CPU use.
- Implement Argon2 or PHC parsing in RustFlow: rejected; use the maintained
  RustCrypto library.
- Spawn one blocking verification task per request without a permit: rejected
  because it permits unbounded queued KDF work.
- Automatically rewrite a legacy credential during verification: rejected
  because the identity port has no user-store transaction or persistence
  contract.

## Verification

Tests cover correct and incorrect Argon2id verification, strict version/cost
encoding, malformed/oversized encodings, legacy PBKDF2 verification, explicit
rehash signaling, and the static provider's verification concurrency cap.
`cargo audit` and independent security review are required before closing the
grouped Phase 2 work.
