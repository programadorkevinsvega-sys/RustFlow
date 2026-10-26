# ADR 0004: React-like component authoring and reactive UI contract

- **Status:** Proposed for implementation design; the product direction is accepted
- **Date:** 2026-10-26
- **Scope:** Public component API, reactivity ergonomics, view macro, and browser renderer
- **Related:** `ROADMAP.md`, `docs/PRODUCT_SCOPE.md`, `docs/VIEW_MODEL.md`, `docs/REACTIVITY_RUNTIME.md`

## Context

RustFlow already has a typed reactive runtime, a structural view model, the `view!` procedural macro, a native component lifecycle/registry model, an initial WASM DOM adapter, and an SSR serializer. These pieces do not yet compose into the intended React-like everyday authoring experience. In particular, the current public flow uses `Runtime::signal`, component descriptors and manual registry APIs; ordinary expression interpolation is not automatically reactive; the ergonomic `signal(value)`, `Signal::update`, and `#[component]` APIs are not implemented; and reactive DOM updates currently replace the mounted range.

The product objective is a familiar declarative and reactive component workflow in Rust. This does not mean React source/API compatibility or a drop-in replacement.

## Decision

Adopt the following **product acceptance contract** for the public UI experience:

1. **Ergonomic state API:** component authors can create local state using an API conceptually like `signal(initial)` and can read, set, and update it without manually constructing a `Runtime` in every component. Ownership and runtime isolation must remain explicit and testable.
2. **Owner-bound lifecycle:** signals, effects, subscriptions, and cleanup belong to an explicit component/owner scope. A hidden process-global mutable runtime singleton is not an acceptable shortcut.
3. **Reactive interpolation:** ordinary signal-backed children such as `{count}` in `view!` track dependencies and update automatically. The common case must not require a manual `@dynamic` wrapper.
4. **Typed function components:** a `#[component]`-style API creates component descriptors/metadata from normal Rust component functions. Normal usage should not require users to write `ComponentDefinition` setup plumbing or manually register every component.
5. **Typed props and callbacks:** component props have compile-time type checking and actionable diagnostics; callback props can be passed from parent to child. DOM event handlers remain typed callbacks and are removed on owner disposal.
6. **Fine-grained rendering:** signal updates patch affected DOM nodes/regions rather than replacing the entire mounted range. Reconciliation preserves DOM/component identity when type and keys indicate the same logical child; removing/reordering keyed children follows a documented contract.
7. **Runnable evidence:** the intended Counter and todo examples compile using the public API, run in a supported browser, and are covered by E2E tests for updates, callbacks, keyed/conditional behavior, accessibility basics, and cleanup.
8. **Familiar Rust API vocabulary:** use recognizable React-inspired names in idiomatic Rust `snake_case`—`use_state`, `use_effect`, `use_memo`, `use_ref`, and `use_context`—where RustFlow supports those concepts. Familiar names are a discoverability goal, not a promise of React's exact runtime semantics or signatures.
9. **Documentation accuracy:** code samples using the future contract remain prominently labeled as proposed until compile tests and browser tests prove them. Roadmap items stay open until those gates pass.

## Illustrative design target (not current API)

```rust
use rustflow::prelude::*;

#[component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);

    view! {
        <div class="counter">
            <p>"Contador: " {count}</p>
            <button on:click={move |_| count.update(|n| *n += 1)}>
                "Sumar"
            </button>
            <input prop:value={count} on:input={move |e| count.set(e.value())} />
        </div>
    }
}

#[component]
fn App() -> impl IntoView {
    view! {
        <main>
            <Counter initial={0} />
        </main>
    }
}
```

This snippet defines intent only. It does not compile against the current repository API and must not be presented as a working example.

## Design questions to resolve before implementation

- How does component invocation establish the active owner/runtime context so that `signal(initial)` is ergonomic without an implicit process-global singleton?
- How are errors from view construction represented while preserving a concise component return type such as `impl IntoView`?
- What is the exact event type for DOM events and callback props, including extracting input values without exposing untyped browser internals?
- What are the semantics when typed props change after initial mount, and how does that interact with component-local signal state?
- How do child component identity, list keys, cleanup, and panic/error paths interact during partial DOM updates?
- Which minimum browser(s) are supported for the first end-to-end vertical slice?

### React-inspired vocabulary and the selected state model

RustFlow will follow the **signal-first model** for state and DOM updates, while borrowing familiar React vocabulary for concepts developers recognize. It will not implement state cells indexed by hook-call order as the default model.

| Familiar concept | RustFlow target name | Intended contract and current status |
| --- | --- | --- |
| `useState` | `use_state(initial)` | Planned, component/owner-scoped convenience backed by the signal model. It must not require positional hook-cell ordering. Whether it returns the signal handle directly or another typed wrapper must be settled before implementation. |
| Signal primitive | `signal(initial)` | Planned lower-level reactive primitive with `get`, `set`, and `update`, bound to the current explicit owner/runtime scope. |
| `useEffect` | `use_effect(...)` | Planned ergonomic wrapper over scope-owned, dependency-tracked effects. Dependencies are intended to be tracked from signal reads, rather than requiring a React-style dependency array by default. Cleanup on rerun and on scope disposal must be specified and tested. |
| `useMemo` | `use_memo(...)` | Planned memoized derived computation with dependencies tracked according to RustFlow's reactive semantics, not copied hook-cell behavior. |
| `useRef` | `use_ref(initial)` | Planned stable, component-scoped storage for non-reactive values or DOM handles; mutation alone should not trigger rendering. |
| `useContext` | `use_context::<T>()` | Planned typed lookup from a nearest provider in the component-owner tree. Provider boundaries and missing-provider behavior must be explicit. |
| `onClick` | `on:click` | The `view!` macro already supports this RustFlow event syntax. |
| `className` | `class` | Use HTML naming in `view!`; do not add a JavaScript-specific alias without a demonstrated need. |
| `key` | `key` | A stable child identity marker already exists in the view model; reconciliation remains pending. |

The current reactivity implementation has `Runtime::signal`, `Scope::effect`, and owner scopes, but it does **not** yet expose the component-facing `use_state`, `use_effect`, `use_memo`, `use_ref`, or `use_context` façade. Treat all listed future names as proposed until compile tests and actual runtime tests exist.

The design intentionally combines familiar naming with signal semantics:

- A component is normally initialized once per mount; changing a signal invalidates the parts of the view that read it, rather than requiring the entire component function to re-execute for every state update.
- Ordinary signal interpolation should bind directly to the reactive graph.
- Structural changes, such as replacing an element type or inserting/removing/reordering keyed children, are handled by a view diff/reconciliation step.
- Rendering/diff planning should be separated from the commit step that mutates browser DOM.
- Root event delegation may be used for supported bubbling events, with direct-listener fallbacks for events whose semantics require them. The supported event contract and cleanup behavior must be tested.
- React Fiber-style interruptible scheduling and React's positional hook-cell rules are **not** MVP requirements. Add comparable scheduling complexity only if measured use cases justify it.

The exact public names, signatures, error types, and lifecycle details must be settled before implementation and shown in compiling examples. Any API that uses familiar names must document RustFlow-specific semantics rather than suggesting drop-in compatibility.

The implementation proposal must answer the design questions above, include alternatives and trade-offs, and be accompanied by compile tests and browser-level evidence.

## Alternatives considered

- **Keep the low-level API as the normal authoring surface:** rejected as the sole experience because it requires runtime/registry plumbing and fails the product ergonomics target.
- **Copy React's hook cells and call-order rules:** not selected for the default model; RustFlow's signal graph already supports dependency tracking without positional state cells. Familiar names may still be used with RustFlow-defined signal/owner semantics.
- **Copy every React helper before validating use cases:** rejected. Names such as `use_memo`, `use_ref`, and `use_context` are roadmap targets, but each helper still needs a typed contract, lifecycle semantics, and tests.
- **Use a hidden process-global runtime singleton:** rejected because it risks unclear ownership, reentrancy, isolation, and server/request leakage.
- **Add convenience syntax without reactive renderer behavior:** rejected because JSX-like syntax alone would still produce stale children or full-range replacement and would not satisfy the observable UI contract.
- **Implement the entire full-stack API before testing a vertical slice:** rejected because the key browser experience should be proven with a Counter and todo before broadening scope.

## Consequences

- Phase 2 remains complete only for the native structural view/component model; this ADR does not retroactively claim that the public ergonomic contract exists.
- Phase 3 now owns delivery and verification of the ergonomic API, fine-grained renderer, browser bundle, Counter, todo, and browser E2E gates.
- SSR/hydration, client navigation, forms integration, and server-side state isolation remain separate roadmap work; this ADR does not mark them implemented.
- Before closing Phase 3, tests and public docs must demonstrate the intended user workflow. A WASM compile check or native runtime unit tests alone are insufficient.


## Implementation status (not an acceptance claim)

The Phase 3 renderer work has started on a separate implementation branch. Its first reconciliation slice updates compatible text and element nodes in place and reuses compatible nodes with matching keys in keyed lists, but only where the full tree contains no deferred dynamic evaluator or component descriptor. Opaque subtrees still use the conservative full-range replacement path. This does **not** satisfy the Phase 3 exit gate: the owner-bound ergonomic API, function-component macro/typed callback props, automatically reactive interpolation, complete lifecycle-aware reconciliation, browser bundle, runnable counter/todo, browser E2E suite, bundle measurement, and debugging feedback remain unverified or unimplemented until their individual checks pass.