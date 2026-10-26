# Reactivity Semantics Contract (Phase 0.4)

**Status:** Proposed contract; implementation has not started.  
**Purpose:** Establish observable behavior for the first RustFlow reactivity prototype before selecting internal data structures.

This contract applies to the initial single-threaded reactive runtime. It does not promise API stability or define browser DOM integration.

## 1. Core concepts

- **Signal:** owns a typed value that can be read and written.
- **Effect:** executes a user-provided computation and records signals read during that execution.
- **Owner scope:** groups effects and child scopes so their work and resources can be disposed together.
- **Scheduler:** coordinates effect reruns caused by signal writes.

The initial prototype should favor explicit, testable semantics over premature optimization.

## 2. Signal reads and writes

1. Reading a signal inside a running effect records a dependency from that effect to the signal.
2. Reading a signal outside an effect returns its current value and does not create a dependency.
3. Writing a signal changes its stored value. If the value is equal to the previous value according to the signal's documented equality policy, the write does not schedule dependent effects.
4. The initial equality policy should be documented and deterministic. The prototype may use PartialEq where available; types without equality must have an explicit policy rather than silently claiming change suppression.
5. A write must not synchronously corrupt the dependency graph, even if it occurs while an effect is running.

## 3. Effect execution and dependencies

1. An effect runs once when created, unless a future explicitly documented lazy API is introduced.
2. Dependencies are the signals actually read during the most recent completed execution.
3. Before rerunning an effect, its old dependencies are detached; reads in the new execution establish the replacement set. This prevents stale dependencies when control flow changes.
4. If an effect reads no signals, later signal writes do not rerun it.
5. Nested effects have independent dependency sets and belong to an explicit owner scope. Parent reruns must not accidentally duplicate child effects; exact child recreation behavior must be decided by the owner/lifecycle API and covered by tests before implementation.
6. An effect must not be concurrently executed by the initial runtime.

## 4. Scheduling, batching, and re-entrancy

1. Multiple writes may mark the same effect dirty, but one scheduler flush must not enqueue duplicate executions of that effect while it is already pending.
2. Effects run to completion; a scheduler must not recursively enter the same effect.
3. Writes performed by an effect are allowed, but scheduling must be deterministic and must not cause uncontrolled recursive stack growth.
4. A batch groups writes so observers see the final values at the flush boundary. Each affected effect runs at most once per batch unless it is invalidated again by a later, distinct flush.
5. The initial ordering guarantee is limited: effects execute in deterministic FIFO order of first becoming pending within a flush. If implementation constraints require a different order, amend this contract and test it before relying on ordering publicly.
6. Cycles must not hang indefinitely. The prototype must detect or bound repeated scheduling and report a documented runtime diagnostic. It must not silently claim that every cyclic graph has a meaningful fixed point.

## 5. Ownership, disposal, and failures

1. Every effect belongs to an owner scope, either explicitly supplied or created by a documented root-scope API.
2. Disposing a scope is idempotent, detaches its effects from dependencies, cancels pending runs, and disposes owned child scopes and registered cleanup callbacks.
3. A disposed effect never runs again. Attempts to use a disposed scope must produce documented behavior; they must not resurrect disposed effects.
4. Cleanup callbacks run at most once and in a deterministic order. The exact order (reverse registration order is preferred) must be implemented and tested.
5. A panic during an effect must not leave the graph permanently half-linked or the scheduler permanently marked as running. The prototype should restore internal invariants and propagate or report the panic according to a documented policy.
6. The initial contract does not promise recovery from arbitrary user-code panics or cross-thread use.

## 6. Threading and runtime boundaries

- The initial runtime is single-threaded and intended for native tests and later browser/WASM use.
- Send/Sync guarantees are not implied. Do not add unsafe code to simulate thread safety.
- Server-side concurrent rendering is outside this contract until a separate runtime/isolation decision is documented.
- DOM updates, components, macros, SSR, and hydration are out of scope for this contract.

## 7. Required verification before the contract is accepted

- Signal reads/writes and initial effect execution.
- Only dependent effects rerun.
- Equal-value writes follow the documented equality policy.
- Dynamic dependencies replace stale dependencies.
- Nested effects and owner scope behavior.
- Disposal before and after an effect is queued; repeated disposal.
- Batching, duplicate queue suppression, FIFO scheduling, and re-entrant writes.
- Cycle diagnostics and panic-path graph/scheduler invariants.
- No dependency tracking outside a running effect.
- Documentation examples compile and tests are deterministic.

## 8. Decisions intentionally left open

The implementation PR must make and document a choice for:
- Public API shape and whether signal handles are cloneable.
- Equality bounds and behavior for values without PartialEq.
- Whether effects return handles and how explicit disposal interacts with scopes.
- Cleanup registration API and cleanup ordering.
- Batch API shape and nested-batch behavior.
- Exact cycle threshold/diagnostic.
- Panic propagation versus error capture, consistent with Rust's unwind configuration.

These choices should be informed by a minimal prototype and tests, not by undocumented assumptions.

## Acceptance gate

This document is a proposed contract until reviewed. Phase 0.4 is complete only when the contract is reviewed, open decisions are resolved or explicitly deferred with rationale, and the roadmap reflects that status. The implementation must not be marked complete by this design document alone.
