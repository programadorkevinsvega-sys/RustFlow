# Structural View and Component Contracts (Phase 2 prototype)

The view model is target-independent. It represents UI intent; it is not rendered HTML and is not a sanitization boundary.

## Structural nodes

- Text stores its content verbatim.
- Element stores a syntax-validated element name, attributes, DOM properties, event callbacks, children, and an optional stable key.
- Fragment groups ordered children without adding an element.
- Component stores a validated component type name, typed properties, children, and an optional key. A component host resolves the descriptor into a reusable definition.
- Dynamic stores a deferred evaluator. Reactive adapters must evaluate it inside an effect owned by the component's reactive scope.
- KeyedList preserves child order and rejects duplicate keys.

## Validation and error behavior

Element names allow an ASCII letter followed by ASCII letters, digits, or hyphens. Attribute and property names are separately validated. Inline event attributes and direct raw-HTML sinks (for example, srcdoc, innerHTML, and outerHTML) are rejected. Duplicate attributes, properties, event names, and keyed-list identities return typed ViewError values.

This is syntactic validation, not an HTML allowlist or URL sanitization policy. Attribute values may still include unsafe URLs, and the tree must never be treated as safe HTML. Any HTML or DOM renderer must escape text and attribute values, apply an explicit URL policy, and avoid raw HTML insertion by default.

## Attributes, properties, and events

Attributes use AttributeValue::Text or AttributeValue::Boolean; a renderer should omit boolean attributes whose value is false and emit an empty boolean attribute for true. DOM properties use a separate PropertyValue enum. Events use named EventHandler values rather than on* string attributes. An event callback receives a normalized EventPayload; the concrete browser adapter is responsible for converting native events into that payload and unregistering listeners.

## Component lifecycle

The rustflow facade exposes ComponentDefinition and ComponentInstance. Setup runs once per mount and receives a ComponentContext with a dedicated reactive Scope. Setup may create signals/effects in that scope and register cleanup callbacks. It returns a render callback that must be pure: it receives current properties and children, and returns a replacement View. Calling update computes the next view before changing stored inputs; if rendering fails, the previous view is retained. Unmount is idempotent and disposes the scope, effects, descendants, and registered cleanup callbacks. Updates after unmount return a typed error.

ComponentInstance produces a replacement view tree; it does not itself mutate a DOM. A platform renderer must use the Renderer contract to mount, update, and unmount that tree. Context/provider semantics remain deferred until a concrete component use case requires them.

## Declarative macro

The rustflow-macros crate exposes rustflow::view! with a constrained HTML-like grammar. Tags must close correctly; children can be Rust literals or brace-delimited expressions; plain attributes, boolean attributes, prop:name values, on:event handlers, keyed nodes using key={value}, @dynamic { expression }, and uppercase component descriptors are supported. It returns Result<View, ViewError>. Parser errors use a source span and compile_error! diagnostic. See rustdoc examples and compile-fail checks for the supported contract.
