# Reactivity Runtime Prototype (Phase 1.2)

This document describes the first implemented API in `rustflow::reactivity`. The API is a prototype and is not yet covered by a stability guarantee.

## Quick start

```rust
use rustflow::reactivity::{Runtime, RuntimeError};

fn main() -> Result<(), RuntimeError> {
    let runtime = Runtime::new();
    let count = runtime.signal(0_i32)?;
    let read_count = count.clone();
    let doubled = count.clone();
    let effect = runtime.effect(move || {
        println!("count = {}, doubled = {}", read_count.get(), doubled.get() * 2);
    })?;

    count.set(1);
    runtime.batch(|| {
        count.set(2);
        count.set(3);
    })?;
    effect.dispose()?;
    runtime.dispose()?;
    Ok(())
}
```

## API and behavior

- `Runtime::signal(value)` creates a typed signal that suppresses writes when `old == new`, using Rust's `PartialEq` semantics, including floating-point `NaN != NaN`.
- `Runtime::signal_eq(value)` is an explicit alias for `signal`.
- `Runtime::signal_always_changed(value)` is the fallback for values without `PartialEq`; every write notifies subscribers.
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

## Planned ergonomic component API (not implemented)

RustFlow's product target is to let component authors use an API conceptually like `signal(initial)`, followed by `get()`, `set(value)`, and `update(|value| ...)`, without manually creating a `Runtime` in each component. This is a design target, not part of the current public API.

The implemented API remains `Runtime::new()`, `Runtime::signal(value)`, and `Signal::get/set`; `Signal::update` and the standalone component-facing `signal(value)` helper are not currently available. See [ADR 0004](adr/0004-react-like-component-authoring.md) for the target experience and design questions.

Any ergonomic façade must preserve explicit ownership and isolated runtime state. It must not be implemented as an undocumented process-global mutable runtime singleton. Reads inside view expressions must participate in documented dependency tracking once the renderer integration is implemented.

## Current limitations

This prototype does not provide async effects, tracking across `await`, cross-thread notifications, the ergonomic component-facing `signal(value)` façade, the `Signal::update` helper, automatic reactive interpolation in `view!`, the `#[component]` API, fine-grained DOM integration, SSR hydration, or automatic scope disposal from `Drop`. The lower-level component model and an initial DOM adapter exist, but they are not yet the integrated React-like authoring experience. Cleanup callbacks and effects are synchronous. With `panic = "abort"`, Rust cannot recover from user panics.
