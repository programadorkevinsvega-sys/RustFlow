# ADR 0004: React-like component authoring and reactive UI contract

- **Status:** Accepted for the Phase 3 MVP; implementation merged and verified in `main` on 2026-10-26
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

## Phase 3 implementation decisions

- **Ownership:** component setup runs in an owned reactive scope; signals, effects, and cleanup remain scope-bound rather than using a process-global mutable runtime singleton.
- **Errors and events:** the browser adapter exposes recoverable render errors through `ReactiveMountedDom::take_error`; DOM handlers and component callback props use typed callback APIs, including `EventHandler`.
- **Prop updates:** `ComponentInstance::preview_update` evaluates new props/children without recreating setup state; the candidate is accepted only after the DOM plan commits successfully.
- **Identity and cleanup:** compatible component identity is determined by component name and key. Keyed component descriptors inherit their list key, compatible instances preserve state/DOM identity, and replaced or removed instances dispose owned resources.
- **Browser evidence:** production-bundle Playwright E2E runs in Chromium and covers Counter identity, prop updates, keyed todo reordering/CRUD, replacement cleanup, and conditional mount/unmount.

These decisions close the Phase 3 MVP contract only. `use_memo`, `use_ref`, and `use_context`, SSR/hydration, and the wider server/framework roadmap remain separate work.

### React-inspired vocabulary and the selected state model

RustFlow will follow the **signal-first model** for state and DOM updates, while borrowing familiar React vocabulary for concepts developers recognize. It will not implement state cells indexed by hook-call order as the default model.

| Familiar concept | RustFlow target name | Intended contract and current status |
| --- | --- | --- |
| `useState` | `use_state(initial)` | Implemented for the Phase 3 MVP as an owner-scoped convenience backed by signals; it returns `Signal<T>` and does not use positional hook-cell ordering. |
| Signal primitive | `signal(initial)` | Implemented for the Phase 3 MVP with `get`, `set`, and `update`, bound to the active owner/runtime scope. |
| `useEffect` | `use_effect(...)` | Implemented for the Phase 3 MVP as a scope-owned, dependency-tracked effect. Cleanup on rerun and on scope disposal is covered by tests. |
| `useMemo` | `use_memo(...)` | Planned memoized derived computation with dependencies tracked according to RustFlow's reactive semantics, not copied hook-cell behavior. |
| `useRef` | `use_ref(initial)` | Planned stable, component-scoped storage for non-reactive values or DOM handles; mutation alone should not trigger rendering. |
| `useContext` | `use_context::<T>()` | Planned typed lookup from a nearest provider in the component-owner tree. Provider boundaries and missing-provider behavior must be explicit. |
| `onClick` | `on:click` | The `view!` macro already supports this RustFlow event syntax. |
| `className` | `class` | Use HTML naming in `view!`; do not add a JavaScript-specific alias without a demonstrated need. |
| `key` | `key` | Implemented in the view model and browser reconciliation; keyed list reorders preserve compatible child identity and replacement/removal disposes obsolete instances. |

The Phase 3 implementation in `main` exposes the owner-bound `signal`, `use_state`, and `use_effect` APIs and verifies them with runtime and browser tests. `use_memo`, `use_ref`, and `use_context` remain planned and must not be described as available until their own contracts and tests exist.

The design intentionally combines familiar naming with signal semantics:

- A component is normally initialized once per mount; changing a signal invalidates the parts of the view that read it, rather than requiring the entire component function to re-execute for every state update.
- Ordinary signal-identifier interpolation binds to the reactive graph; arbitrary expressions that read signals still require an explicit dynamic region.
- Structural changes, including element replacement and keyed insertions/removals/reorders, are handled by a view diff/reconciliation step.
- Rendering/diff planning is separated from the commit step that mutates browser DOM.
- Supported bubbling events are delegated at the mount boundary, with direct-listener fallbacks where required; listener cleanup is tied to the owning region/component.
- React Fiber-style interruptible scheduling and React's positional hook-cell rules are **not** MVP requirements. Add comparable scheduling complexity only if measured use cases justify it.

The supported Phase 3 public names, signatures, error behavior, and lifecycle details are documented in the component guide and covered by compile/runtime/browser tests. New helpers and broader lifecycle semantics require separate contracts and evidence. RustFlow documents its own signal/owner semantics and does not claim drop-in React compatibility.

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
- Phase 3 was closed only after its acceptance evidence was integrated into `main` and the post-merge CI passed on the exact main commit, including Rust stable/1.85, WASM, bundle budget, browser E2E, documentation, and governance checks.


## Implementation status (merged into `main`; Phase 3 closed)

The current stacked implementation branch (PR #26, based on PR #25) now includes the owner-bound `signal`/`use_state`/`use_effect` API, the constrained `#[component]` macro with typed props and callback props, reactive signal-identifier interpolation, a production WASM/Vite bundle, Counter and todo examples, browser E2E coverage, a raw-WASM size budget, and basic browser error/source-map feedback.

### Component props without resetting setup state

The component macro separates one-time setup statements from the render callback. Stateful APIs must be direct top-level setup statements; the generated render callback reads the current typed props on every render. `ComponentInstance::preview_update` computes a candidate view from new props and children without recreating the owner scope, and `commit_preview_update` accepts that candidate only after the host has successfully committed its DOM plan. A compatible component therefore keeps its existing signals/effects while refreshing values derived from props.

### Diff/evaluation and DOM commit

`crates/rustflow-dom/src/browser.rs` now builds an explicit `ReconciliationPlan` before applying DOM mutations. The evaluation phase validates the candidate tree and prebuilds inserted/replaced subtrees off-DOM inside `PreparedReplacement`; its owned resources are cleaned if the plan is discarded. The commit phase consumes the plan, patches compatible text/elements and child ranges, moves prepared nodes into place, and disposes obsolete component/dynamic regions and their scopes, effects, listeners, and cleanups. Component compatibility is based on name and key, not equality of changing props. `KeyedChild::new` propagates the list key to a root component descriptor so keyed component identity follows the list key.

Regression coverage includes a parent changing a visible child prop while its local signal and compatible DOM nodes survive, keyed reordering preserving local child state, changing a component's key replacing its DOM and resetting its local state while cleaning the old instance, row removal cleanup, and structural conditional mount/unmount.

PR #26 was merged into the `feat/phase-3-browser-runtime` branch, and PR #25 was then merged into `main` on 2026-10-26. The post-merge CI completed successfully on commit `43f5fb9c12996ea600b47930553a9f91dc6055e9` with all seven checks green. Phase 3 is therefore closed in `main`. This closure does not imply that `use_memo`, `use_ref`, `use_context`, SSR/hydration, or other later-phase deliverables are complete.
