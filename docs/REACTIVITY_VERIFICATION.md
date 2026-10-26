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
