# Reactivity Semantics Contract (Phase 1.1)

**Status:** Accepted contract (Phase 1.1). Runtime implementation exists in `crates/rustflow/src/reactivity.rs`; correctness and performance gates are tracked separately in `ROADMAP.md` and `docs/REACTIVITY_VERIFICATION.md`.  
**Purpose:** Define observable behavior for the first RustFlow reactivity prototype before choosing internal data structures.

This contract applies to the initial single-threaded reactive runtime. It does not promise API stability and does not define browser DOM integration, components, SSR, or hydration. Normative terms **MUST**, **MUST NOT**, and **SHOULD** express requirements for the prototype.

## 1. Core concepts

- **Signal:** owns a typed value that can be read and written through a handle.
- **Effect:** runs a user computation and tracks the signals read during its latest execution.
- **Owner scope:** owns effects, child scopes, and cleanup callbacks.
- **Scheduler:** queues invalidated effects and runs them at flush boundaries.
- **Runtime:** owns the graph, scheduler, and root-scope context. Independent runtimes MUST NOT share dependency state.

The prototype prioritizes deterministic, testable semantics over optimization.

## 2. Signal reads, writes, and dependency tracking

1. A signal read returns its current value. When an effect is actively collecting dependencies, the read records a dependency between that effect and the signal.
2. Reads outside an effect do not create dependencies. Reads during cleanup do not create dependencies for the effect being cleaned up.
3. Dependency collection is scoped to the currently executing effect. A nested effect temporarily becomes the active collector; after it finishes, the parent collector is restored.
4. A write stores the new value before scheduling dependents. It MUST NOT synchronously run dependent effects in the middle of the write.
5. The initial equality policy is PartialEq: if old and new values compare equal, the write is a no-op and does not notify dependents. Types without PartialEq use an explicitly documented always-changed policy; the runtime MUST NOT imply equality suppression for them. The implementation must document how floating-point NaN behaves under the chosen PartialEq comparison.
6. A signal write invalidates each currently subscribed effect at most once while it remains pending. Writes during an effect execution may invalidate that effect, but the effect is scheduled for a later scheduler turn, never recursively entered.
7. Handles are runtime-bound. Cross-runtime reads/writes MUST produce a documented error or panic; they MUST NOT silently connect two runtime graphs.

## 3. Effects, nested effects, dynamic dependencies, and cleanup

1. An effect runs once when created. There is no lazy-effect behavior in the initial API.
2. The dependency set after a successful run is exactly the set of signals read during that run. Before rerunning, old subscriptions are detached; the new set is collected as the effect executes. If execution fails, internal graph invariants must be restored and the documented failure policy applies.
3. Conditional reads are dynamic. If an effect reads signal A on one run and signal B on the next, it is no longer subscribed to A after the second run completes.
4. Effects are not recursively re-entered. A write during an effect can queue another run after the current run returns.
5. Nested effects have their own dependency sets and are owned by the currently active owner scope, not implicitly by the parent's dependency set. Re-running a parent does not duplicate its child effects: child effects persist until explicitly disposed or their owner scope is disposed. Code that needs a fresh child lifecycle MUST create and dispose an explicit child scope for that lifecycle.
6. Cleanup callbacks are registered with an owner scope. They run at most once, in reverse registration order (LIFO), when that scope is disposed. Cleanup runs before owned effects/children are fully released, and the implementation must document the exact disposal sequence and test it. Cleanup MUST NOT resurrect disposed resources.
7. Effect execution is synchronous and runs to completion. Async effects and implicit tracking across await are out of scope; users must explicitly bridge async work in a future API.

## 4. Scheduling, batching, re-entrancy, and cycles

1. Effects are invalidated by writes and queued for a scheduler flush. A pending-set prevents duplicate queue entries.
2. Effects execute in FIFO order based on when they first become pending in a flush. If an effect is invalidated while already pending, its queue position does not change.
3. Writes outside an explicit batch request a flush at the next runtime-controlled flush boundary. The initial prototype exposes an explicit batch operation; writes nested inside a batch do not flush until the outermost batch exits. Nested batches are supported and coalesce into that outermost boundary.
4. At a batch boundary, each affected effect runs at most once for the pending work it observed at the start of its run. If its execution causes further writes, those writes may enqueue it again for a subsequent scheduler turn in the same overall flush. This continues until the queue is empty or the cycle guard trips.
5. The scheduler MUST NOT recursively enter an effect or grow the call stack proportional to reactive update depth. Scheduler turns are drained iteratively.
6. Self-writes are allowed but are subject to cycle detection. The prototype uses a finite per-flush execution limit; exceeding it stops the flush and reports a cycle diagnostic containing at least the limit and that the graph did not quiesce. The initial implementation should choose and test a conservative fixed default (proposed: 1,000 effect executions per flush). This is a safety guard, not a claim that all valid graphs converge.
7. On a cycle diagnostic, the runtime must restore scheduler bookkeeping to a usable state and define whether remaining work is cleared or retained. The prototype should clear the current flush queue, return/report the diagnostic, and permit a later independent flush. It MUST NOT silently loop forever.
8. Ordering is deterministic for a given sequence of writes and effect creation. Consumers MUST NOT depend on a stronger topological ordering than FIFO invalidation order.

## 5. Ownership scopes and deterministic disposal

1. Every effect belongs to an owner scope. The runtime provides a root scope and a way to create child scopes explicitly.
2. Child scopes are disposed before their parent scope's effects; effects and child scopes are disposed in reverse creation order. Cleanup callbacks run in LIFO order within their scope. The implementation must make the full order explicit in API docs and tests.
3. Disposing a scope is idempotent. It marks the scope disposed, prevents new owned work from being scheduled, cancels pending effects, detaches subscriptions, disposes child scopes, and runs registered cleanup callbacks exactly once.
4. A disposed effect never runs again. Creating an effect under a disposed scope fails deterministically. Disposing a parent disposes all descendants.
5. Scope/effect handles are intended to be lightweight ownership controls. Their exact Rust types may evolve, but they MUST NOT permit use-after-dispose to resurrect subscriptions.
6. Cleanup panics must not prevent the runtime from attempting remaining cleanup actions. The implementation must document panic handling and preserve internal bookkeeping even if the panic is propagated after cleanup.

## 6. Threading, browser behavior, and server guarantees

1. The first runtime is single-threaded. Its runtime, signal handles, effects, and scopes are not Send or Sync unless a future design explicitly changes and tests that guarantee.
2. Browser/WASM usage is single-threaded and follows the same signal, effect, scheduling, batching, and disposal semantics. Browser event callbacks may write signals; they do not cause recursive effect execution.
3. The initial runtime makes no server-side concurrent-rendering or cross-request sharing guarantee. A server integration must create an isolated runtime per render/request or explicitly establish an equivalent isolation boundary before concurrent use.
4. No unsafe implementation may be used merely to claim thread safety. Shared mutable state across threads, async execution, and cross-thread notifications are out of scope.
5. DOM updates, components, macros, SSR, hydration, and async effects are out of scope for this contract.

## 7. Failure and diagnostic behavior

1. The implementation must choose one consistent panic policy compatible with the crate's unwind configuration. Under unwind builds, a panic must not leave the active-effect pointer, dependency graph, pending-set, or scheduler-running flag corrupted.
2. The initial recommendation is to propagate user effect panics after restoring runtime invariants; do not silently swallow them. Cleanup should continue where feasible, then propagate the first panic according to Rust's unwind behavior.
3. Invalid runtime/scope use and cycle-guard trips must have deterministic, documented diagnostics. Error messages are not a stable API in the prototype, but error categories and recovery behavior are.
4. panic=abort builds cannot promise recovery from user panics; document this limitation.

## 8. Required verification

Tests must cover at least:

- Reads/writes inside and outside effects, including no tracking outside a running effect.
- Initial effect execution and only-dependent-effect reruns.
- Equal-value writes and the no-PartialEq policy.
- Dynamic dependency replacement and stale-subscription removal.
- Nested effects with independent dependencies and no accidental duplication on parent rerun.
- Cleanup order, exactly-once cleanup, and cleanup during disposal.
- Disposal before and after an effect is queued; repeated disposal; attempts to create work in disposed scopes.
- FIFO scheduling, duplicate queue suppression, nested batches, re-entrant writes, and self-writes.
- Cycle-limit diagnostics and scheduler usability after a cycle.
- Panic paths that restore graph and scheduler invariants.
- Runtime isolation and compile-time assertions for intended Send/Sync behavior.
- Deterministic tests and compiling documentation examples.

## 9. Accepted design decisions

The maintainer has accepted the following recommendations for the prototype:

- Equality: PartialEq when available; always-changed semantics otherwise.
- Effects: eager initial execution; dynamic dependencies replace the prior set after each run.
- Nested effects: owned by explicit/current owner scope and persist across parent reruns unless their child scope is disposed.
- Scheduling: FIFO, iterative drain, pending-set deduplication, nested batch coalescing.
- Cycles: finite execution guard, proposed default 1,000 executions per flush, diagnostic and queue reset.
- Disposal: descendant scopes first, reverse creation order, cleanup callbacks LIFO.
- Panics: restore invariants, attempt remaining cleanup, then propagate under unwind builds.
- Threading: non-Send/non-Sync single-thread runtime; server use requires runtime isolation.

These decisions define the Phase 1.1 contract. Implementation details may evolve only if they preserve these observable guarantees or a documented contract revision is reviewed before relying on changed behavior.

## Acceptance gate

Phase 1.1 semantics are accepted. The five Phase 1.1 roadmap items may be marked complete because the contract is now explicit and accepted. This does not complete Phase 1.2 implementation or later correctness/performance gates. The implementation must not be marked complete by this document alone.
