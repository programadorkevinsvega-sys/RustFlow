
# ADR 0010: SSR page and layout composition

- **Status:** Proposed for review
- **Date:** 2026-10-27
- **Phase:** 5.2 — SSR composition and route data

## Context

Phase 5.1 established the component execution-target contract, bounded hydration JSON, and the static SSR fallback rules. The HTTP adapter already exposed fragment/document helpers, but applications had to manually compose route component descriptors and wrapper layouts at each route. SSR must continue to render a useful document before browser execution and must not turn route data into an implicit global context.

## Decision

Add `rustflow_server::HtmlPage` as an application-level composition helper. It accepts an existing structural view or a named page component with explicit `Property` values and children. Its `with_layout` method wraps the body in a registered layout component, allowing nested layouts while preserving the existing structural view model. Layout calls wrap the current body; call the innermost layout first and the outermost layout last. `render` delegates document creation and HTML serialization to the existing safe SSR helpers.

Route-to-component data flow is explicit. Route handlers extract, validate, and project only public fields from the `Request`; they pass those values into the page component's properties. The helper does not receive the complete request and does not automatically serialize arbitrary Rust structs, headers, cookies, sessions, authorization data, or private application state. The application remains responsible for data validation and the public/private allow-list.

Component definitions and registry entries may be reused across requests, but component-owned reactive state may not. Every SSR render creates a fresh runtime, mounts each component in an owned scope, serializes the resulting view, and unmounts/disposes the scope before returning. No process-global mutable runtime or request state is introduced. A renderer can be reused as a definition/configuration holder; its per-render state is not reused.

## Consequences

- Page content and nested layouts use the same registered component model as browser components.
- Full documents are serialized through the existing escaping, element allow-list, attribute policy, and URL-scheme checks; arbitrary raw HTML strings are not accepted.
- Layout composition is explicit and order-dependent, and the route handler explicitly selects component properties.
- Each request gets isolated reactive state and deterministic output for the same structural tree, registered definitions, and props, provided render callbacks are themselves deterministic and side-effect free.
- Component props are not a hydration transport. DOM hydration, boundary IDs, client navigation, and mismatch recovery remain in Phases 5.3 and 5.4.

## Validation

- Test route-facing component properties rendered through two nested layout components in a complete escaped HTML document.
- Reuse one SSR renderer across multiple renders with different and repeated props; assert request-local signal values, deterministic HTML for repeated inputs, and cleanup on every render.
- Keep existing checks for browser-only SSR fallbacks, target-capability guards, safe document serialization, Rust/Clippy/tests/docs, WASM dependencies, and browser E2E.
