# ADR 0036: MongoDB SRV resolver evaluation

- **Status:** Accepted; defer SRV implementation to a separate approved change
- **Phase:** 2.11 - MongoDB SRV evaluation
- **Date:** 2026-11-21
- **Related:** ADR 0018, ADR 0021

## Context

RustFlow currently pins MongoDB Rust driver 3.10.1 with
`default-features = false`, enabling only `compat-3-0-0` and `rustls-tls`.
The optional `dns-resolver` feature provides `mongodb+srv://` and pulls
Hickory's `hickory-net`, `hickory-resolver`, and `hickory-proto` 0.26.x
dependencies. SRV must not be enabled as part of this evaluation.

## Evaluation

- The MongoDB 3.10.1 crate is Apache-2.0, declares Rust 1.88 MSRV, and remains
  compatible with RustFlow's declared Rust 1.94 minimum. Its released manifest
  specifies Hickory 0.26 dependencies when `dns-resolver` is selected.
- The `hickory-net` 0.26.0 release is affected by
  `RUSTSEC-2026-0120` / `GHSA-3v94-mw7p-v465`; the advisory marks 0.26.1 and
  later as patched. Hickory 0.26.1 is MIT OR Apache-2.0 and declares Rust 1.88
  MSRV; 0.26.3 is the latest inspected 0.26 patch release.
- The advisory's unbounded NSEC3 loop is reachable with Hickory DNSSEC
  validation features enabled. MongoDB's `dns-resolver` feature declares the
  resolver dependencies but does not itself enable Hickory's DNSSEC feature.
  Keep the lockfile check explicit because feature unification can change the
  resolved graph.
- The current repository lockfile contains the MongoDB driver but no Hickory
  packages. `MongoConfig` rejects `mongodb+srv://`; existing parser tests also
  prove that standard multi-host `mongodb://` remains supported.
- The MongoDB driver is maintained by MongoDB and Hickory is maintained in the
  Hickory DNS project. Both inspected driver and resolver license metadata are
  compatible with the workspace MIT OR Apache-2.0 policy.
- No evaluation test requires an Atlas account, credential, or external DNS
  service. This evaluation did not enable the feature or perform network SRV
  resolution.

## Decision

Keep SRV disabled for this phase. Defer feature enablement to a separately
approved implementation change that must:

1. Enable only the resolver feature needed by the supported MongoDB driver.
2. Resolve and audit the exact Hickory dependency graph, requiring
   `hickory-net >= 0.26.1` and confirming no DNSSEC feature is activated
   unintentionally.
3. Keep the current Rust 1.94 workspace MSRV and review license/advisory output.
4. Add deterministic local tests for URI parsing and resolver configuration;
   do not require Atlas or public DNS in CI.
5. Update the public connection guide and project status before advertising
   `mongodb+srv://`.

The current supported scheme remains `mongodb://` with explicit hosts.

## Consequences

SRV connection strings remain rejected even though a patched resolver release
is available. That is an intentional scope boundary, not a claim that the
current Hickory 0.26 line is still wholly affected by the earlier advisory.
The exact patched transitive resolution and runtime DNS behavior must be
verified in the future implementation change.

## Verification

- `cargo info mongodb@3.10.1`: Apache-2.0, Rust 1.88, optional `dns-resolver`.
- MongoDB 3.10.1 source manifest: optional Hickory dependencies use 0.26.
- `cargo info hickory-net@0.26.1` and `@0.26.3`: MIT OR Apache-2.0, Rust 1.88.
- `RUSTSEC-2026-0120`: patched at `hickory-net >= 0.26.1`.
- The current `Cargo.lock` contains no `hickory-*` packages while the optional
  resolver feature is disabled.
- `cargo test -p rustflow-server --test mongodb_adapter` covers SRV rejection
  and standard multi-host parsing; execution is recorded with the grouped
  phase verification.
