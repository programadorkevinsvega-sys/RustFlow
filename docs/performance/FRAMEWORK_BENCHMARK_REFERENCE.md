# Competitive framework benchmark reference

**Status:** upstream audit/source pin and executable workflow merged; the official comparison has not yet been run or accepted.  
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

The source audit and exact pin are complete. PR #78 now contains the following implementation scaffolding:

- The repository is tracked as submodule `benchmarks/js-framework-benchmark` at `fa510a76ffe066af1d30f3e75fa51d3fcbc14009`.
- The selected keyed upstream implementations are React 19.2.0, Vue 3.5.39 and Svelte 5.42.1, built from their pinned upstream lockfiles.
- RustFlow has a keyed browser implementation under the official path `/frameworks/keyed/rustflow/`, using the benchmark's expected controls (`run`, `runlots`, `add`, `update`, `clear`, `swaprows`), keyed row IDs, selection class and remove action.
- The Phase 8.1 browser workflow checks out the submodule, builds the selected upstream frameworks, serves the RustFlow WASM build beside them, verifies the actual Chromium version is `156.0.8078.4`, and invokes the upstream Playwright runner. The CPU aggregate is generated from the upstream raw total/script/paint trace results.
- For the dedicated PR #78, the sequence is standard RustFlow browser measurements, same-runner A/B/A, official four-framework comparison, then the RustFlow-only extended list stress suite. The extended stress workload therefore does not precede and contaminate the A/B/A or cross-framework samples.

**Execution evidence is still pending.** A wired workflow, a compiled fixture or a test source file is not accepted measurement evidence. The latest HEAD must pass the WASM build, browser E2E, A/B/A comparator, full extended suite and official four-framework runner, and must publish complete raw artifacts before this phase can be accepted.

### Known coverage gaps in the pinned upstream runner

- The pinned `benchmarkInfos` array includes CPU, memory and size benchmarks but does not include the declared Lighthouse startup main benchmark in its normal executable list. It does not produce a comparable official startup/Lighthouse result through the invoked runner. RustFlow initialization timing remains a separate specialized metric until a separate equivalent startup protocol is implemented.
- The pinned source comments out the memory IDs `23_update5-memory` and `24_run5-memory`; the deterministic patcher re-enables the two metadata entries and their Playwright implementations, including an exact five-update postcondition. This makes the same upstream `measureUserAgentSpecificMemory()` protocol exercise five updates and five replacements for all four frameworks.
- The executable runner's row-count and warmup differences from the README are documented above. The upstream-compatible aggregate uses the executable runner's behavior; 10,000-row extended semantics remain separate.

The competitive CPU aggregate is valid only when each of the nine official operation IDs has non-empty samples from React, Vue, Svelte and RustFlow collected by the same CI job and browser. Compute each ratio as `median(RustFlow) / median(framework)` and aggregate the nine ratios with a geometric mean. Do not mix memory, bundle or startup measurements into that CPU index.

Keep Phase 8.1 open until the complete current-HEAD evidence is reviewed, all required checks pass and the result shows the actual state against every framework. Do not declare superiority from the internal RustFlow A/B/A comparison.

## Phase 8.1 comparative extension

The nine original upstream IDs above remain unmodified and are still run as a separate upstream-compatible result set. The Phase 8.1 requirements additionally call for a 10,000-row partial update, appending 1,000 rows to a 10,000-row table and clearing 10,000 rows. The pinned upstream executable does not provide those exact row counts. PR #78 therefore applies a small, deterministic, version-controlled patch to the pinned upstream runner:

| Added ID | Setup | Measured operation |
|---|---|---|
| `10_update10th10k` | keyed 10,000-row table | Update every tenth row |
| `11_append1k-after10k` | keyed 10,000-row table | Append 1,000 rows (11,000 final rows) |
| `12_clear10k` | keyed 10,000-row table | Clear all 10,000 rows |

All four frameworks run these three operations using the same upstream Playwright/CDP trace pipeline, warmup count and measured-run count in the same GitHub Actions browser job. They are included in a separate three-operation extension geometric mean and in a combined 12-operation CPU geometric mean. The upstream-only nine-operation metric remains available for compatibility with published benchmark data. The patcher aborts unless each exact source anchor appears exactly once, preventing silent application to an unexpected upstream revision.

The same deterministic patch re-enables the pinned upstream memory cases for five partial updates and five full replacements. They are executed on all four frameworks after the standard memory scenarios and remain separate from latency samples.

## Same-runner startup extension

The official upstream CPU/MEM/SIZE runner does not include its declared Lighthouse startup main benchmark in the ordinary `benchmarkInfos` list. PR #78 therefore adds a separate Playwright startup pass inside the same browser job after the upstream CPU, memory and size runs. It performs three discarded warmups and 15 measured cold-page navigations per framework in a rotated sequential order, recording navigation milestones, first paint/FCP, resource transfer/encoded bytes, and the first real `#run` interaction through a 1,000-row DOM commit. RustFlow's explicit runtime-init mark is recorded as an implementation-specific field. Parse/compile/evaluate breakdowns are not inferred from navigation timing and remain a separate profiling task.
