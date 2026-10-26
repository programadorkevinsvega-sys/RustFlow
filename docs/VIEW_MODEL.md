# Structural View and Component Contracts (Phase 2 prototype)

The view model is target-independent. It represents UI intent; it is not rendered HTML and is not a sanitization boundary.

## Structural nodes

- Text stores its content verbatim.
- Element stores a syntax-validated element name, attributes, DOM properties, event callbacks, children, and an optional stable key.
- Fragment groups ordered children without adding an element.
- Component stores a validated component type name, typed properties, children, and an optional key. `ComponentRegistry` resolves a descriptor to a reusable `ComponentDefinition`; recursive traversal and reconciliation of nested descriptors belong to a renderer.
- Dynamic stores a deferred evaluator. Reactive adapters must evaluate it inside an effect owned by the component's reactive scope.
- KeyedList preserves child order and rejects duplicate keys.

## Keyed-list identity

Keys are stable string or integer identities supplied by the caller. They must be unique among siblings in a single list snapshot. Reusing the same key across updates declares that the logical child is the same even when its position changes; removing a key means that child is removed. The current model validates each snapshot but does not diff snapshots or preserve mounted DOM/component instances. That reconciliation belongs to the Phase 3 renderer.

## Validation and error behavior

Element names allow an ASCII letter followed by ASCII letters, digits, or hyphens. Attribute and property names are separately validated. Inline event attributes and direct raw-HTML sinks (for example, srcdoc, innerHTML, and outerHTML) are rejected. Duplicate attributes, properties, event names, and keyed-list identities return typed ViewError values.

This is syntactic validation, not an HTML allowlist or URL sanitization policy. Attribute values may still include unsafe URLs, and the tree must never be treated as safe HTML. Any HTML or DOM renderer must escape text and attribute values, apply an explicit URL policy, and avoid raw HTML insertion by default.

## Attributes, properties, and events

Attributes use AttributeValue::Text or AttributeValue::Boolean; a renderer should omit boolean attributes whose value is false and emit an empty boolean attribute for true. DOM properties use a separate PropertyValue enum. Events use named EventHandler values rather than on* string attributes. An event callback receives a normalized EventPayload; the concrete browser adapter is responsible for converting native events into that payload and unregistering listeners.

## Component lifecycle

The rustflow facade exposes ComponentDefinition and ComponentInstance. Setup runs once per mount and receives a ComponentContext with a dedicated reactive Scope. Setup may create signals/effects in that scope and register cleanup callbacks. It returns a render callback that must be pure: it receives current properties and children, and returns a replacement View. Calling update computes the next view before changing stored inputs; if rendering fails, the previous view is retained. Unmount is idempotent and disposes the scope, effects, descendants, and registered cleanup callbacks. Updates after unmount return a typed error.

ComponentInstance produces a replacement view tree; it does not itself mutate a DOM. A platform renderer must use the Renderer contract to mount, update, and unmount that tree. Recursive component traversal and keyed reconciliation are not implemented by the registry. Context/provider semantics remain deferred until a concrete component use case requires them.

## Declarative macro

The rustflow-macros crate exposes rustflow::view! with a constrained HTML-like grammar. Tags must close correctly; children can be Rust literals or brace-delimited expressions; plain attributes, boolean attributes, prop:name values, on:event handlers, keyed nodes using key={value}, @dynamic { expression }, and uppercase component descriptors are supported. It returns Result<View, ViewError>. Parser errors use a source span and compile_error! diagnostic. See rustdoc examples and compile-fail checks for the supported contract.

## React-like authoring target (planned, not implemented)

The current model provides structural nodes and generic component descriptors; it is not yet the normal-use API RustFlow is aiming for. The target developer experience is a typed function component with owned local signals, automatic reactive interpolation, typed props/callbacks, and automatic component descriptor/registration integration.

Illustrative design target only; this snippet is **not compilable against the current API**:

```rust
#[component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);

    view! {
        <div class="counter">
            <p>"Count: " {count}</p>
            <button on:click={move |_| count.update(|n| *n += 1)}>
                "Add"
            </button>
        </div>
    }
}
```

For routine signal-backed children, `{count}` is intended to track and update automatically without requiring `@dynamic`. Component props and callbacks should be typed. Normal component use should not require manually constructing a `ComponentDefinition` and registering it in a `ComponentRegistry`.

The implementation must define how a signal is bound to the active component/owner scope without introducing an undocumented process-global mutable runtime. It must also settle error propagation from `view!`, prop-update behavior, callback event types, and cleanup guarantees. These requirements are tracked in [ADR 0004](adr/0004-react-like-component-authoring.md) and the Phase 3 exit gate in `ROADMAP.md`.
