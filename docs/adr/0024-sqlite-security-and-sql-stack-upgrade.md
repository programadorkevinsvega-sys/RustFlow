# ADR 0024: Remediate bundled SQLite advisory and update the SQL stack

- Status: Accepted for implementation; final compatibility and CI verification pending.
- Date: 2026-11-13.
- Related: ADR 0016 (SQLx pools and migrations), ADR 0017 (MySQL/MariaDB driver), ADR 0019 (database support matrix), ADR 0020 (SQL ORM contract), ADR 0022 (relation loading).

## Context

RustFlow's resolved SQLx 0.8.6 dependency graph bundles SQLite 3.46.0 through `libsqlite3-sys 0.30.1`. RustSec advisory RUSTSEC-2026-0459 affects the bundled SQLite sources through `libsqlite3-sys 0.38.2`; the fixed line starts at 0.39.0 and requires Rust 1.88. The issue can expose uninitialized page-cache memory when a process opens a database together with a crafted sibling WAL file. This is a local database-file trust-boundary risk, not evidence of a remotely reachable HTTP exploit.

The published SQLx 0.9.0 release still constrains `libsqlite3-sys` below 0.38.0. SeaORM 1.1 uses SQLx 0.8 and cannot share SQLx 0.9 pool handles. The upstream SQLx development revision below widens the SQLite dependency range to `<0.40.0`, admits the patched crate, and uses the SQLx 0.9.0 package version. SeaORM 2.0.4 is the compatible published ORM release and requires Rust 1.94.

## Decision

- Raise the workspace MSRV from Rust 1.85 to Rust 1.94 and use Rust 1.94 plus stable in CI. Update generated server and CRUD projects to declare the same minimum.
- Upgrade the shared SQL driver to SQLx 0.9.0 and SeaORM to 2.0.4 so both use the same SQLx pool types.
- Until SQLx publishes a release containing the fixed SQLite dependency range, patch every SQLx crate used by the workspace to the exact upstream commit `987b8c5a74b6da8b71da6b5549925a7f6e59aac9` in `transact-rs/sqlx`. Pinning a full SHA avoids tracking a moving branch. Replace this temporary source patch with a released SQLx version once it admits `libsqlite3-sys >=0.39.0`.
- Put the same exact SQLx patch in both generated application manifests. Cargo patches are applied by the consuming workspace root; a patch in RustFlow's workspace is not inherited automatically by applications depending on RustFlow from Git. Without this generated-project patch, the CLI's dependency graph could resolve SQLx 0.9.0 from crates.io and retain the vulnerable SQLite range.
- Resolve and lock `libsqlite3-sys 0.39.1`, which bundles SQLite 3.54.1. Keep the existing SQLite file and WAL trust boundary; do not claim that changing the dependency makes untrusted database files a safe application input.
- Keep `cargo audit --deny warnings` enabled and remove the obsolete `RUSTSEC-2023-0071` ignore once the regenerated graph no longer contains `rsa 0.9.10`. Do not suppress RUSTSEC-2026-0459.
- Preserve the one-pool SQLx/SeaORM boundary, the four-engine SQL contract, and MongoDB's independent adapter. Revalidate typed pools, migrations, codecs, relation loading, generated CRUD, and backend error mapping after the major ORM/SQLx change.

## Alternatives considered

- Keep SQLx 0.8.6 or use only the published SQLx 0.9.0: rejected because neither dependency range can resolve the patched `libsqlite3-sys`.
- Use only the system SQLite library: rejected as a complete fix because the vulnerable crate remains in the resolved dependency graph and `cargo audit` still reports the advisory.
- Ignore the RustSec advisory: rejected because it would hide a real bundled-library vulnerability.
- Move to SQLx's unreleased source without a full SHA: rejected because a branch or tag could move.
- Wait for a published SQLx fix: deferred as the fallback if the pinned upstream revision fails compatibility or security validation; no released SQLx version found at decision time admits the patched SQLite dependency.

## Consequences and limitations

- RustFlow users must use Rust 1.94 or newer. This is a compatibility change for users on Rust 1.85–1.93 and applies to generated projects too. Generated manifests carry the pinned SQLx source patch until a suitable release exists.
- Until an upstream release is available, the SQLx source is a pinned development commit rather than a crates.io release. Its provenance, license, dependency graph, and exact SHA must remain visible and be reviewed again when replacing it.
- The SeaORM 1.1 to 2.0 and SQLx 0.8 to 0.9 upgrades can affect codecs, query generation, entity macros, migrations, and transaction behavior. Existing four-engine contracts are required gates; the version bump alone is not proof of compatibility.
- SQLite is expected to resolve to the patched bundled 3.54.1 source. The runtime version assertion and `cargo audit` must pass on the final PR head before this ADR is considered verified.

## Documentation and verification plan

Public documentation impact is required: update the workspace/toolchain requirements, generated-app minimum, database version matrix, SQL driver/ORM versions, SQLite runtime version, and limitations. Update the technical ORM/database contracts and current status without rewriting historical PR verification records.

GitHub CI must verify the exact final commit with the Rust 1.94 and stable workspace lanes, `cargo audit --deny warnings`, formatting, Clippy, all workspace tests, all configured SQL/MongoDB contracts, generated CLI integration, Rustdoc, WebAssembly/browser checks, documentation build, governance, and dependency review. No successful result is claimed until observed on that commit.
