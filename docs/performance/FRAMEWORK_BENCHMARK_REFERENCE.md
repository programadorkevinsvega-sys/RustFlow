# Competitive framework benchmark reference

**Status:** upstream audit and source pin completed; the comparative suite is not implemented or accepted yet.  
**Audit date:** 2026-11-03  
**Repository:** [krausest/js-framework-benchmark](https://github.com/krausest/js-framework-benchmark)  
**Pinned source commit:** [`fa510a76ffe066af1d30f3e75fa51d3fcbc14009`](https://github.com/krausest/js-framework-benchmark/commit/fa510a76ffe066af1d30f3e75fa51d3fcbc14009)  
**Pinned commit timestamp:** 2026-11-02T19:06:47Z  
**Official result release:** Chrome 156 (the latest published release observed during this audit).

This pin is a reproducibility boundary. Do not replace it with a floating `master` checkout during CI. The official results page is useful as a published reference, but its timings must not be mixed with RustFlow measurements because its hardware, run count, and execution conditions are not the same as the RustFlow GitHub Actions runner.

## Canonical keyed implementations selected

Chrome 156 is a keyed-only official release. The following directories are the representative implementations selected from the pinned source tree:

| Framework | Upstream directory | Framework version from that directory's package lock | Notes |
|---|---|---:|---|
| React | `frameworks/keyed/react-hooks` | React 19.2.0; React DOM 19.2.0 | Reducer-driven updates; memoized keyed rows |
| Vue | `frameworks/keyed/vue` | Vue 3.5.39 | Keyed `v-for`, shallow state container and memoized rows |
| Svelte | `frameworks/keyed/svelte` | Svelte 5.42.1 | Keyed each block and Svelte 5 runes |

The corresponding upstream lockfiles are `package-lock.json` files in those directories (lockfileVersion 3). Build tool versions pinned by those lockfiles include:

| Implementation | Build tooling versions |
|---|---|
| React | webpack 5.97.1, Babel core 7.26.0, Babel React preset 7.26.3 |
| Vue | Vite 7.0.5, `@vitejs/plugin-vue` 6.0.0 |
| Svelte | Rollup 4.28.1, `rollup-plugin-svelte` 7.2.2, `@rollup/plugin-node-resolve` 15.3.0 |

The version strings in the package manifests are not sufficient provenance by themselves: use the exact lockfiles from the pinned upstream commit when building the selected applications. Do not silently update framework versions independently of the reference pin.

## The nine upstream CPU operation identifiers

The pinned executable runner defines these identifiers in `webdriver-ts/src/benchmarksCommon.ts` and implements their browser actions in `webdriver-ts/src/benchmarksPlaywright.ts`:

| ID | Operation | Executable fixture semantics in the pinned source |
|---|---|---|
| `01_run1k` | Create rows | Create 1,000 rows |
| `02_replace1k` | Replace all rows | Replace the 1,000-row table |
| `03_update10th1k_x16` | Partial update | Update every tenth row in a 1,000-row table |
| `04_select1k` | Select row | Select/highlight one row in a 1,000-row table |
| `05_swap1k` | Swap rows | Swap the two fixed rows in a 1,000-row table |
| `06_remove-one-1k` | Remove row | Remove one row from a 1,000-row table |
| `07_create10k` | Create many rows | Create 10,000 rows |
| `08_create1k-after1k_x2` | Append rows | Start from 1,000 rows and append 1,000 (2,000 final rows) |
| `09_clear1k_x8` | Clear rows | Clear a 1,000-row table |

### Upstream README versus executable runner

The pinned README is a useful high-level description, but it is not fully aligned with the executable benchmark implementation at this commit. In particular:

- The README describes `03_update10th1k_x16` as updating a 10,000-row table; the pinned Playwright implementation calls `#run`, asserts 1,000 rows, and then updates every tenth row in that table.
- The README describes appending 1,000 rows to a 10,000-row table; the pinned `benchAppendToManyRows` primes 1,000 rows and verifies 2,000 rows after append.
- The README describes clearing 10,000 rows; the pinned `benchClear` primes and clears 1,000 rows.
- The README labels some create/clear operations as having no warmup, while the pinned benchmark metadata and setup code perform warmup actions. The partial-update setup uses three explicit warmup updates; other benchmark setup paths have their own warmup behavior.

The source locations used to resolve these conflicts are:

- [README.md at the pinned commit](https://github.com/krausest/js-framework-benchmark/blob/fa510a76ffe066af1d30f3e75fa51d3fcbc14009/README.md)
- [CPU benchmark metadata](https://github.com/krausest/js-framework-benchmark/blob/fa510a76ffe066af1d30f3e75fa51d3fcbc14009/webdriver-ts/src/benchmarksCommon.ts)
- [Executable Playwright operations](https://github.com/krausest/js-framework-benchmark/blob/fa510a76ffe066af1d30f3e75fa51d3fcbc14009/webdriver-ts/src/benchmarksPlaywright.ts)
- [Playwright benchmark runner and trace timing](https://github.com/krausest/js-framework-benchmark/blob/fa510a76ffe066af1d30f3e75fa51d3fcbc14009/webdriver-ts/src/forkedBenchmarkRunnerPlaywright.ts)

Do not silently reconcile this discrepancy by changing the upstream benchmark's row counts or warmups and then labelling the result "official". The implementation must preserve two explicit layers:

1. **Upstream-compatible suite:** reproduce the pinned executable runner's actual operations, data shapes, row counts, and setup/measurement boundaries, and record the README/source discrepancies in reports.
2. **RustFlow Phase 8.1 extended suite:** add the extra requirements from the RustFlow roadmap, including partial updates over 10,000 rows, appending 1,000 rows to a 10,000-row table, and clearing 10,000 rows. These are extended scenarios, not substitutions for upstream identifiers.

Both layers must use equivalent keyed semantics and equivalent data/DOM structures across all four frameworks. Report them under distinct identifiers and never blend the extended scenarios into the official-compatible aggregate.

## Measurement behavior to preserve

The pinned Playwright runner:

- Navigates to each selected implementation and waits for `networkidle`.
- Executes the benchmark-specific setup before collecting the CPU trace.
- Requests a major synchronous GC before CPU timing.
- Uses Chromium DevTools tracing categories for user timing and browser timeline work.
- Applies the upstream CPU throttling factors when enabled: 4x for partial update, select, swap and clear; 2x for remove.
- Runs the actual button/row interaction, waits for a browser frame interval, then computes total duration, script duration and paint duration from the trace.
- Measures memory in a separate path using `performance.measureUserAgentSpecificMemory()` after forced GC; memory passes must not be mixed into latency samples.

The RustFlow harness may add stronger controls (same job, same runner, sequential framework execution, randomized/rotated order, repeated rounds, raw samples, p50/p90/p95), but it must not redefine an upstream operation. Record the browser engine and exact version observed at runtime; do not label Playwright's bundled Chromium as Google Chrome.

## RustFlow comparison contract

- Use the same GitHub Actions job and runner instance for React, Vue, Svelte and RustFlow.
- Pin the upstream source commit, selected framework lockfiles, Node version, Playwright version and browser revision.
- Use production builds and the same viewport, data generation, CSS, row markup, attributes, keys, and interaction targets.
- Run the four implementations sequentially. Do not allow concurrent benchmark jobs to share the runner during a measurement pass.
- Keep initial load/setup and measurement intervals explicit. DOM assertions and fixture verification must happen after the timed operation unless the official runner includes them in its tracing protocol.
- Preserve raw samples, round identifiers, execution order, framework commits, exact dependencies, CPU/RAM metadata, OS/kernel, Rust version and browser version in the JSON artifact.
- Report script, paint and total duration where the tracing protocol provides them. Do not infer Rust/WASM execution time from JavaScript main-thread duration.
- Compute RustFlow/framework ratios per operation from the same paired execution conditions. Keep the required Phase 8.1 geometric-mean index separate from the official site's own weighted geometric mean.
- Never import public Chrome 156 leaderboard times as if they were measured on the RustFlow runner.

## Acceptance status

The upstream repository and selected lockfiles are pinned, and the RustFlow keyed benchmark page plus the same-runner GitHub Actions harness are now part of PR #78:

- Submodule `benchmarks/js-framework-benchmark` points to `fa510a76ffe066af1d30f3e75fa51d3fcbc14009`.
- The selected upstream keyed implementations are built from their pinned lockfiles: React 19.2.0, Vue 3.5.39 and Svelte 5.42.1.
- RustFlow's WASM build is copied into the official runner as `keyed/rustflow`; its controls preserve the upstream button IDs and keyed row lifecycle.
- The official CPU suite is enabled for PR #78, manual dispatch and weekly scheduled runs. It requires Chromium `156.0.8078.4`, runs all four implementations sequentially on the same browser job, and preserves raw result files plus a geometric-mean comparison report.
- The extended RustFlow-only suite runs after A/B/A and the upstream comparison on PR #78, manual dispatch and weekly scheduled runs, keeping its extra workload out of earlier timing measurements.

**Still pending acceptance:** the current HEAD has not yet produced successful CI evidence for the keyed RustFlow fixture, A/B/A stability with the updated warmups, all nine comparative operations, the extended 10k-row checks, memory and size outputs. The startup/Lighthouse startup benchmark is not part of the pinned runner's ordinary CPU/MEM/SIZE list, and upstream memory cases for five updates and five replacements are commented out in the pinned source. These are explicit gaps, not completed measurements.

The suite does not establish competitive superiority until a complete same-runner artifact contains all four frameworks and all nine CPU operation identifiers. The geometric mean is calculated only from those paired CPU samples; memory and bundle results remain separate. Phase 8.1 stays open until the artifacts have been reviewed and the required checks pass.
