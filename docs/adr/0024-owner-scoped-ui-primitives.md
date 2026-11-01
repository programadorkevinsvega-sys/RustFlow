# ADR 0024: Owner-scoped UI derivations, references, and typed contexts

- **Status:** Proposed for review; implementation is present on the Phase 7.2 pull-request branch only.
- **Date:** 2026-11-01
- **Phase:** 7.2 — UI completeness
- **Scope:** memoized derivations, stable non-reactive references, typed owner-tree contexts

## Context

RustFlow already has a synchronous, single-threaded reactive runtime built from signals, eager effects, a FIFO scheduler, explicit owner scopes, and LIFO cleanup. Its runtime uses <code>Rc</code> and <code>RefCell</code>; it is intentionally not <code>Send</code>/<code>Sync</code>. Component setup executes inside an owner scope, while rendering is repeatable and must remain free of state-creation APIs.

The runtime did not expose cached derived computations, general owner-scoped mutable storage, or typed context providers. Existing component, DOM, SSR, and view-model boundaries must remain intact. In particular, a browser DOM handle must not leak into the target-independent view crate.

## Decision

### Memoized computations

- Implement <code>Scope::memo</code>, <code>use_memo</code>, and fallible <code>try_use_memo</code> on top of the existing effect scheduler and a normal owner-bound signal.
- Memo computations are eager: execute once during construction and rerun when the current dynamic dependency set invalidates.
- The computation accepts <code>Fn() -&gt; T</code>, returns a cloneable and equality-comparable value, and is expected to be free of side effects.
- <code>PartialEq</code> compares the new result with the previous cached result. Equal results leave the output signal unchanged, so consumers are not notified. This is result equality, not dependency equality.
- A batch coalesces invalidations. The memo is recomputed at the outer flush boundary; reads during the open batch observe the last committed result.
- The normal effect collector replaces stale dependencies after each successful execution. A memo with no reactive dependencies runs once.
- The memo is owned by the creating scope. Disposing that scope disposes its effect and output signal.
- The first computation executes synchronously. If it panics, the newly created effect is disposed before the panic is resumed. If a later computation panics, the existing effect policy detaches the failed run's dependencies and propagates the panic; no error value or fallback is synthesized. Callers that need recoverable computation failure should return a typed <code>Result</code> as the memo value.

### Stable references

- Implement <code>Scope::reference</code>, <code>use_ref</code>, and fallible <code>try_use_ref</code> for stable, non-reactive component-owned storage.
- The storage is a typed handle over <code>Rc&lt;RefCell&lt;Option&lt;T&gt;&gt;&gt;</code>. Read/mutation APIs use short-lived closure borrows; borrow conflicts and disposal are reported as <code>RuntimeError</code>.
- Writes do not participate in signal dependency tracking and do not schedule effects.
- On owner disposal, the value is taken and dropped. Cloned handles remain memory-safe but become invalid and return <code>RefDisposed</code>.
- The API deliberately does not expose raw pointers or long-lived <code>Ref</code>/<code>RefMut</code> guards.
- This decision does not implement DOM element references. Their distinct mount/hydration/reconciliation/unmount lifecycle is specified separately by [ADR 0025: Browser DOM element reference lifecycle](0025-browser-dom-element-reference-lifecycle.md), implemented in the browser adapter without coupling <code>rustflow-view</code> to <code>web_sys</code>.

### Typed contexts

- <code>Context&lt;T&gt;</code> is a typed identity token created with <code>Context::new</code>. Cloning a token preserves its identity. Independently created tokens are different even if they have the same <code>T</code>.
- Do not key provider storage by <code>TypeId</code> alone. The runtime assigns each context token its own monotonically allocated identity and stores values in each owner node under that identity.
- Each owner can provide a context identity once; a nested owner may shadow an ancestor provider.
- Lookup walks the current owner and then its ancestors, returning the nearest value as <code>Rc&lt;T&gt;</code>.
- A missing provider is explicitly <code>Ok(None)</code> (or <code>None</code> from the ergonomic helper). Runtime/owner failures remain errors in <code>try_use_context</code>; the ergonomic helper reports them by panic rather than silently defaulting.
- Context lookup itself is not reactive. Put a <code>Signal&lt;T&gt;</code> in the context when consumers need reactive updates.
- Providers belong to runtime owner trees, not process-global maps. SSR must create isolated runtime/owner trees per render/request; a shared context token is only an identity, never shared request state.
- Values use safe <code>Rc&lt;dyn Any&gt;</code> erasure and <code>Rc::downcast</code>; no unchecked cast is introduced.

### Component setup and hook model

- Export the APIs through the RustFlow prelude and expose provider/consumer operations through <code>ComponentContext</code>.
- Extend the component macro's setup classifier so these owner-scoped constructors are permitted only in top-level setup statements and rejected in the repeatable render body.
- No positional hook slots, call-order indexes, or parallel hidden state store are introduced. All resources are owned by an explicit owner scope.

## Alternatives considered

1. **Positional hook cells:** rejected. This would introduce call-order identity, hidden mutable state, and lifecycle rules foreign to RustFlow's owner/signal architecture.
2. **Use effects as public memo API:** rejected. It exposes scheduling and side effects instead of a read-only derived value.
3. **Global context map keyed by type:** rejected. It cannot distinguish two contexts with the same value type and risks cross-request leakage.
4. **Use <code>TypeId</code> as the sole key:** rejected. Two independent contexts of the same Rust type must remain distinct.
5. **Expose DOM nodes from <code>rustflow-view</code>:** rejected. It couples a target-independent model to browser APIs and cannot represent SSR's lack of a DOM. The separate browser-only handle and lifecycle are specified by [ADR 0025](0025-browser-dom-element-reference-lifecycle.md).
6. **Use unsafe pointers for refs:** rejected. Safe scoped storage is used for Rust values; browser nodes use the separately specified mount lifecycle in [ADR 0025](0025-browser-dom-element-reference-lifecycle.md).

## Consequences

- Derived values reuse the current scheduler, dependency collector, signal equality policy, and owner disposal.
- Result comparison requires <code>T: PartialEq</code>. Memos are eager rather than lazy; a dependency invalidation can run computation even if nobody reads the result afterward.
- A memo computation should be pure. The Rust type system cannot prevent interior side effects.
- Refs are single-threaded, like the runtime. They are not synchronization primitives.
- Context identity is independent of the value type and provider lookup is deterministic by the owner ancestry.
- A value cloned out of a context may outlive its provider as a normal <code>Rc</code>; it is no longer discoverable through the disposed owner tree. Applications must not use that clone as proof that the provider remains mounted.
- DOM element refs are not introduced by this ADR; their separate <code>ElementRef</code> contract is defined in [ADR 0025](0025-browser-dom-element-reference-lifecycle.md). Reactive context replacement/notification, component error boundaries, and async resources remain out of scope.

## Verification strategy

- Unit tests count memo executions for initial run, repeated reads, dependency changes, dynamic branch changes, equal derived outputs, batching, no-dependency computations, owner disposal, and failed initial computation.
- Unit tests check ref identity, non-reactive writes, borrow-safe access, and invalidation after disposal.
- Context tests check nearest provider, nested shadowing, duplicate registration, same-type distinct identities, missing values, signal-bearing values, and separate runtimes.
- The component macro, native workspace, WebAssembly, SSR, and browser checks must pass on the final pull-request commit.
- Documentation must state explicitly which behavior is absent. This ADR covers memo, Rust storage refs, and typed contexts only; browser DOM refs are governed separately by ADR 0025. It does not approve component error boundaries or async rendering.
