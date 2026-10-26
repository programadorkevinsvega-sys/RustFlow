# Structural View Model (Phase 2.1 prototype)

This document records the contract that exists in `rustflow-view` today. It is deliberately narrower than the full view/component representation planned by the roadmap.

## Implemented contract

- `View::Text(String)` stores text content exactly as provided.
- `View::Element(Element)` stores an element-name string and an ordered vector of child views.
- `View::Fragment(Fragment)` stores an ordered vector of child views and preserves the fragment boundary.
- Constructors collect child iterators eagerly into owned vectors. Empty elements and fragments are allowed.
- Structural nodes implement `Clone`, `Debug`, `PartialEq`, and `Eq`, supporting deterministic unit tests.
- The `rustflow` facade exposes these types through `rustflow::view`.

## Explicit non-goals and limitations

This model does not yet support component nodes, attributes, DOM properties, boolean attributes, events, dynamic children, keyed identity, lifecycle/cleanup, an HTML renderer, a DOM renderer, SSR, or hydration. It is not connected to the reactive runtime.

Element names are stored without validation. The tree is not serialized or emitted as HTML by this crate. Do not treat it as safe rendered markup or use this prototype as an HTML sanitization boundary. Future renderers must define supported tags and escape text and attributes by default under the security rules in `ARCHITECTURE_RULES.md`.

The public API is pre-1.0. These types establish a minimal data-model boundary; they do not mean that Phase 2 is complete.
