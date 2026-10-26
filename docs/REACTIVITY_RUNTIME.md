# Reactivity Runtime Prototype (Phase 1.2)

This document describes the first implemented API in `rustflow::reactivity`. The API is a prototype and is not yet covered by a stability guarantee.

## Quick start

```rust
use rustflow::reactivity::Runtime;

let runtime = Runtime::new();
let count = runtime.signal_eq(0_i32)?;
let doubled = count.clone();
let effect = runtime.effect(move || {
    println!("count = {}, doubled = {}", count.get(), doubled.get() * 2);
})?;

count.set(1);
runtime.batch(|| {
    count.set(2);
    count.set(3);
})?;
effect.dispose()?;
runtime.dispose()?;
# Ok::<(), rustflow::reactivity::RuntimeError>(())
```

## API and behavior

- `Runtime::signal(value)` creates a typed signal with always-changed write semantics. Use it for values without `PartialEq`.
- `Runtime::signal_eq(value)` creates a typed signal that suppresses writes when `old == new`. It uses Rust's `PartialEq` semantics, including the usual floating-point `NaN != NaN` behavior.
- `Signal::try_get` and `Signal::try_set` return `RuntimeError`; `get` and `set` panic with that diagnostic for ergonomic prototype usage.
- Effects run eagerly on creation. Reads performed by an effect replace its prior dependency set on each successful run. Only subscribed effects are invalidated.
- The scheduler uses FIFO invalidation order, deduplicates queued effects, drains iteratively, and does not recursively flush while an effect or batch is active.
- `Runtime::batch` nests and flushes once at the outer boundary. It returns a cycle diagnostic if the scheduler fails to quiesce.
- A runtime defaults to a 1,000-effect-executions-per-flush guard. `Runtime::with_cycle_limit` is available for tests and specialized use.
- Scopes own effects, child scopes, and cleanup callbacks. Dispose a scope explicitly to cancel pending work, detach subscriptions, dispose descendants/effects, and run cleanup callbacks LIFO. Repeated scope disposal is harmless.
- User effect panics are propagated after the active-effect state and subscriptions are restored. Cleanup panics do not prevent remaining cleanups from being attempted; the first panic is then propagated.
- Runtime/signal/scope handles use `Rc` and `RefCell`, and therefore are not `Send` or `Sync`. Create an independent runtime for each server render/request.

## Diagnostics

`RuntimeError` distinguishes a disposed runtime, invalid signal/effect/scope handles, attempts to use disposed scopes, and scheduler cycle detection. Error text is informative but not a stable API in this prototype. The scheduler clears queued work when the cycle limit is exceeded and can be used for later independent updates.

## Current limitations

This prototype does not provide async effects, tracking across `await`, cross-thread notifications, DOM integration, components, SSR, hydration, or automatic scope disposal from `Drop`. Cleanup callbacks and effects are synchronous. With `panic = "abort"`, Rust cannot recover from user panics.
