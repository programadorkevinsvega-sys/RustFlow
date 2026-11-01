# ADR 0025: Browser DOM element reference lifecycle

- Status: Proposed for formal review; Phase 7.2 implementation merged into `main` via PR #70 (2026-11-01; merge commit `506dd6f193c13a0bb941b6f75238d751a2934230`). This status tracks formal architectural approval, not runtime availability.
- Date: 2026-11-01
- Scope: Phase 7.2 browser DOM references

## Context

RustFlow's <code>use_ref</code> is deliberately non-reactive Rust storage owned by a component scope. It is not a browser node handle. The target-independent <code>rustflow-view</code> model must remain independent from <code>web_sys</code>, while browser applications need a safe way for event handlers to obtain a mounted native element.

A DOM reference also needs more than a cell that stores an element: it must not become visible during view construction or SSR, must attach only after a successful mount or validated hydration, must follow reconciliation/replacement, and must be cleared when the owning mounted range is disposed.

## Decision

- Provide a generic target-independent <code>NodeRef&lt;T&gt;</code> and an element builder method <code>Element::with_node_ref</code> in <code>rustflow-view</code>. The view crate stores a type-erased binding and has no dependency on <code>web_sys</code>.
- Export <code>rustflow_dom::ElementRef</code> as <code>NodeRef&lt;web_sys::Element&gt;</code> from the browser-only DOM adapter.
- Support <code>ref={element_ref}</code> on native elements in the <code>view!</code> macro. Component descriptors reject this syntax because a component may render zero, one, or multiple native elements.
- Keep the handle empty while a view is being built, while a mount is detached/prepared, and during SSR. Attach it only after the browser range has been inserted or the existing SSR range has passed hydration validation.
- During reconciliation, keep the handle attached to the same DOM node when the node is compatible, move it to the replacement element when the bound element is replaced, and clear it on removal/unmount. Clear operations compare the actual DOM node identity so cleanup from an older mount cannot erase a newer assignment.
- Permit one reference handle to identify one native element in one mounted region at a time. Duplicate bindings in a view and conflicting mounted attachments are rejected. The handle is non-reactive and reads return a cloned browser handle or <code>None</code>.
- Treat the browser handle as runtime-only. SSR does not serialize it or manufacture a placeholder node, and failed hydration leaves the handle empty.

## Alternatives considered

- Reusing <code>use_ref</code> for DOM nodes: rejected because it conflates ordinary Rust storage with browser lifecycle and encourages DOM values to be retained outside the owning mounted region.
- Adding <code>web_sys</code> to <code>rustflow-view</code>: rejected because it couples the structural model/SSR to the browser target.
- Raw pointers or long-lived <code>Ref</code>/<code>RefMut</code> guards: rejected because they complicate ownership and can expose stale nodes across replacement/unmount.
- Refs on component descriptors: rejected because component output cardinality is not guaranteed to be exactly one element.

## Consequences

- Browser applications can query an <code>ElementRef</code> from a click handler after mount or hydration, then use browser APIs on the cloned element handle.
- A handle does not trigger rendering, subscribe to signals, or guarantee that the node exists; callers must handle <code>None</code>.
- The API is browser/WASM-specific. Server-side code may construct a view carrying the opaque token but receives no DOM element.
- DOM refs are separate from owner-scoped <code>use_ref</code>; they do not introduce positional hook slots or new dependencies.

## Verification

- Unit tests verify handle cloning, stable identity, type checking, and conditional clearing.
- Browser E2E verifies mount, unmount, replacement/remount, and attachment to pre-existing SSR markup after successful hydration.
- Required CI must cover Rust stable/MSRV, WASM, browser E2E, documentation, governance, dependency review, and the site build before phase acceptance.
