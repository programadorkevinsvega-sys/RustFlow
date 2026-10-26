# Reactivity verification and benchmark protocol

This document records the verification strategy for the Phase 1 reactivity prototype. It does not claim that an unchecked gate has passed.

## Correctness verification

Run the same commands locally and in CI:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

The test suite covers dependent-effect invalidation, equality suppression, dynamic dependency replacement, batching, nested effects, scope disposal, cleanup ordering, cycle recovery, cross-runtime access, runtime disposal, re-entrant writes, disposal of queued effects, effect/cleanup panics, runtime isolation, and a deterministic model sequence.

A green test run is necessary but not proof of exhaustive correctness. New regressions should add a focused test first. Property-based testing may be added once the team chooses a dependency and a state-machine model; the deterministic model sequence in the current suite is intentionally dependency-free and is not a substitute for randomized property testing.

## Benchmark workloads

The dependency-free benchmark at `crates/rustflow/benches/reactivity.rs` measures:

- signal reads;
- signal writes;
- scheduling and running one dependent effect;
- a batch that invalidates 256 dependent effects.

Run it with the optimized Cargo bench profile:

```sh
rustc --version
cargo --version
uname -a
RUSTFLOW_BENCH_ITERATIONS=100000 cargo bench -p rustflow --bench reactivity
```

The iteration count can be changed with `RUSTFLOW_BENCH_ITERATIONS`. High-fan-out uses one tenth of the configured count (at least one iteration). The program prints elapsed nanoseconds and average nanoseconds per operation.

## Reproducibility and interpretation

For a publishable baseline, record the full commit SHA, Rust/Cargo versions, OS and kernel, CPU model, power/performance mode where known, iteration count, and the complete output. Run each workload multiple times on an otherwise idle machine and report the median; do not compare results from materially different environments as if they were equivalent. Keep raw runs as artifacts or attach them to the baseline PR.

The current CI runner is useful as a repeatable smoke measurement, not as a stable performance guarantee. No performance claim or Phase 1.4 closure should be made until baseline results have been captured and reviewed. Avoid hard pass/fail nanosecond thresholds until variance has been characterized.

## Initial CI baseline (2026-10-26)

First recorded smoke baseline from the pull-request merge ref for head `a7468c826dc04846dd40c1316f2072df0a59e164` (base `b8b00dfa23ccb9e03f83d72cfeecccc4273a67e0`). This is an initial reference point, not a performance target or release guarantee.

Environment:
- Runner image: GitHub-hosted `ubuntu-24.04`, Ubuntu 24.04.5 LTS.
- Kernel: `6.17.0-1022-azure`, x86_64.
- CPU reported by `lscpu`: AMD EPYC 7763 64-Core Processor, 4 visible vCPUs.
- `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- `cargo 1.99.1 (5f94df478 2026-08-27)`.
- Optimized Cargo bench profile; default `RUSTFLOW_BENCH_ITERATIONS=100000`; three sequential repetitions in one job.

| Workload | Repetition 1 | Repetition 2 | Repetition 3 | Median |
| --- | ---: | ---: | ---: | ---: |
| Signal read (ns/op) | 17.03 | 16.62 | 17.53 | 17.03 |
| Signal write without subscribers (ns/op) | 35.16 | 34.59 | 34.68 | 34.68 |
| Write + one dependent effect (ns/op) | 243.10 | 244.07 | 247.14 | 244.07 |
| Batch flush with 256 dependent effects (ns/batch) | 58944.94 | 58983.92 | 58932.99 | 58944.94 |

These are measurements from a shared virtualized CI runner. They are recorded to make future comparisons possible, but CPU contention, virtualization, host changes, and the simple `std::time` harness can affect results. Use them as a smoke baseline only; make performance decisions from repeated runs on controlled, comparable hardware and consider a statistical benchmark framework if variance or measurement precision becomes important.
