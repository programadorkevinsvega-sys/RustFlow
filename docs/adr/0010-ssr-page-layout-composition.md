
# ADR 0010: SSR page and layout composition

- **Status:** Proposed for review; implementation merged in PR #37, formal architectural approval remains pending
- **Date:** 2026-10-27
- **Phase:** 5.2 — SSR composition and route data

## Context

Phase 5.1 established the component execution-target contract, bounded hydration JSON, and the static SSR fallback rules. The HTTP adapter already exposed fragment/document helpers, but applications had to manually compose route component descriptors and wrapper layouts at each route. SSR must continue to render a useful document before browser execution and must not turn route data into an implicit global context.

## Decision

Add `rustflow_server::HtmlPage` as an application-level composition helper. It accepts an existing structural view or a named page component with explicit `Property` values and children. Its `with_layout` method wraps the body in a registered layout component, allowing nested layouts while preserving the existing structural view model. Layout calls wrap the current body; call the innermost layout first and the outermost layout last. `render` delegates document creation and HTML serialization to the existing safe SSR helpers.

Route-to-component data flow is explicit. Route handlers extract, validate, and project only public fields from the `Request`; they pass those values into the page component's properties. The helper does not receive the complete request and does not automatically serialize arbitrary Rust structs, headers, cookies, sessions, authorization data, or private application state. The application remains responsible for data validation and the public/private allow-list.

Every SSR render creates a fresh runtime, mounts each component in an owned scope, serializes the resulting view, and unmounts/disposes the scope before returning. No process-global mutable runtime or request state is introduced. The shared immutable catalog may be reused concurrently; per-render state is never reused.

The local ComponentRegistry remains suitable for local callbacks but cannot be captured by Application routes because its definitions use Rc-backed callbacks and local View fallbacks. Add SharedComponentDefinition and SharedComponentCatalogBuilder as a separate server-safe path. A built SharedComponentCatalog stores only immutable definitions and Send + Sync setup/fallback factories; it is cheaply cloneable and can be captured by HTTP handlers. Each setup factory produces a local ComponentRender for a request-local reactive runtime and scope. Browser-only fallbacks are created by a Send + Sync factory on demand, so a potentially non-Send View is never stored inside the shared catalog. The runnable `rustflow-server/examples/basic.rs` example and concurrent HTTP integration tests use this path: one catalog is built at startup, captured by route handlers, and reused for distinct and repeated route parameters without state leakage. Each render cleans up its local scopes. The local ComponentRegistry API remains for callbacks that intentionally cannot satisfy the sharing contract; it is not the recommended route-handler pattern.

## Consequences

- Page content and nested layouts use the same registered component model as browser components.
- Full documents are serialized through the existing escaping, element allow-list, attribute policy, and URL-scheme checks; arbitrary raw HTML strings are not accepted.
- Layout composition is explicit and order-dependent, and the route handler explicitly selects component properties.
- Each request gets isolated reactive state and deterministic output for the same structural tree, shared catalog, and props, provided render callbacks are themselves deterministic and side-effect free.
- Shared catalog definitions and fallback factories must be Send + Sync; the local ComponentRegistry remains available where callbacks cannot satisfy that contract.
- Component props are not a hydration transport. DOM hydration, boundary IDs, client navigation, and mismatch recovery were deliberately kept as separate scope and were implemented later in Phases 5.3 and 5.4. This ADR does not install hydration or client navigation by itself.

## Validation

- Test route-facing component properties rendered through two nested layout components in a complete escaped HTML document.
- Reuse one SSR renderer across multiple renders with different and repeated props; assert request-local signal values, deterministic HTML for repeated inputs, and cleanup on every render.
- Keep existing checks for browser-only SSR fallbacks, target-capability guards, safe document serialization, Rust/Clippy/tests/docs, WASM dependencies, and browser E2E.
