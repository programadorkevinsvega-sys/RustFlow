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
8. **Documentation accuracy:** code samples using the future contract remain prominently labeled as proposed until compile tests and browser tests prove them. Roadmap items stay open until those gates pass.

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

The implementation proposal must answer these questions, include alternatives and trade-offs, and be accompanied by compile tests and browser-level evidence.

## Alternatives considered

- **Keep the low-level API as the normal authoring surface:** rejected as the sole experience because it requires runtime/registry plumbing and fails the product ergonomics target.
- **Use a hidden process-global runtime singleton:** rejected because it risks unclear ownership, reentrancy, isolation, and server/request leakage.
- **Add convenience syntax without reactive renderer behavior:** rejected because JSX-like syntax alone would still produce stale children or full-range replacement and would not satisfy the observable UI contract.
- **Implement the entire full-stack API before testing a vertical slice:** rejected because the key browser experience should be proven with a Counter and todo before broadening scope.

## Consequences

- Phase 2 remains complete only for the native structural view/component model; this ADR does not retroactively claim that the public ergonomic contract exists.
- Phase 3 now owns delivery and verification of the ergonomic API, fine-grained renderer, browser bundle, Counter, todo, and browser E2E gates.
- SSR/hydration, client navigation, forms integration, and server-side state isolation remain separate roadmap work; this ADR does not mark them implemented.
- Before closing Phase 3, tests and public docs must demonstrate the intended user workflow. A WASM compile check or native runtime unit tests alone are insufficient.
