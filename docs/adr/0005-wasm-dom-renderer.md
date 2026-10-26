# ADR 0005: Initial WebAssembly DOM adapter

- **Status:** Proposed for review
- **Date:** 2026-10-26
- **Phase:** 3.1–3.2 — Browser runtime

## Context

Phase 2 defines target-independent view nodes, typed attributes/properties/events, component lifecycle, and a renderer contract. Browser code must not leak into reactivity or the view model, and unsafe HTML must not become an accidental security boundary.

## Decision

Introduce the `rustflow-dom` adapter crate. Its browser implementation compiles only for `wasm32-unknown-unknown` and uses `wasm-bindgen`/`web-sys` in that target-specific dependency section. Core and server builds do not depend on browser APIs.

The initial DOM renderer uses comment anchors around the mounted range so fragment roots can be replaced without a visible wrapper. It supports a deliberately constrained allowlist of common semantic elements, text nodes, typed attributes/properties, event callbacks, fragments, keyed-list snapshots, deferred child evaluation on mount/update, and component descriptors registered with `ComponentRegistry`. Event listeners and component scopes are released on update/unmount. `DomRenderer::mount_reactive` binds a view-producing closure to an owned reactive scope; tracked signal changes rerun it, a failed update preserves the previous successful range, and subsequent errors can be retrieved with `ReactiveMountedDom::take_error`. Updates still rebuild the rendered range rather than perform keyed reconciliation or targeted dirty-subtree patching.

The renderer rejects elements outside its allowlist, inline `style` attributes, DOM properties named `style` or `on*`, and URL values whose explicit scheme is not one of `http`, `https`, `mailto`, or `tel`. Relative URLs (including root-relative and fragment URLs) are allowed. URL schemes are normalized by removing ASCII control characters and lowercasing before the scheme check. The `srcset` and `ping` multi-URL attributes are rejected until a dedicated parser can validate each candidate. The renderer's policy is separate from the view model's syntax validation. It is a starting safety boundary, not a complete browser security review; the tree itself must never be treated as safe HTML.

## Alternatives considered

- **Put web-sys in rustflow-view:** couples target-independent structural data to a browser and breaks native/server testing.
- **Put browser APIs in rustflow:** makes the facade target-specific and less explicit.
- **Create rustflow-dom as an adapter:** isolates target-specific APIs and permits native tests for validation policy.

## Consequences and limitations

- The workspace gets a dedicated WASM compile check for the DOM adapter.
- Native workspace tests remain browser-independent.
- This first slice does not complete Phase 3: browser-loadable bundles, keyed reconciliation, targeted dirty-subtree patching, counter/todo examples, E2E tests, and cross-browser verification remain pending.
- SSR, hydration, HTTP server/routing, and CLI are not part of this ADR.
