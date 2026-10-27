# ADR 0010: DOM hydration boundary protocol

- **Status:** Proposed for review
- **Date:** 2026-10-27
- **Phase:** 5.3 — Hydration
- **Related:** ADR 0009, `docs/SSR_RENDERING.md`

## Context

RustFlow already provides an HTML serializer, a validated `HydrationData` JSON container, structural views, a browser DOM renderer with reconciliation, component scopes, and explicit listener cleanup. The missing link is a protocol for identifying existing SSR output and connecting the browser view to that DOM without replacing compatible nodes or enabling events before validation.

The protocol must preserve ordinary SSR compatibility, keep private server data out of the client payload, and fail conservatively when the server DOM cannot be classified.

## Decision

### Opt-in server markers

Normal SSR output is unchanged by default. Call `HtmlRenderer::with_hydration_boundaries()` or use the server page helpers that accept `HydrationData` to opt in to hydration markup.

The initial protocol uses HTML comments, not visible wrapper elements:

- `rustflow:root:start` / `rustflow:root:end` delimit the root view.
- `rustflow:component:start:N` / `rustflow:component:end:N` delimit universal, client-hydratable component output.
- `rustflow:dynamic:start:N` / `rustflow:dynamic:end:N` delimit a dynamic child region.
- `rustflow:static:start:N` / `rustflow:static:end:N` delimit environment-specific or otherwise non-hydratable component output, which the browser leaves static.

N is a zero-based deterministic counter allocated in depth-first render order for component/dynamic/static boundaries. The same view, component catalog, public props, and order must produce the same marker sequence. Markers are structural metadata; they do not grant trust to markup or payload values.

### Public hydration payload

Server helpers reuse `HydrationData` rather than define a second JSON serializer. `HtmlPage::with_hydration_data` opts the response into marker emission and a non-executable `script[type="application/json"]#rustflow-hydration-data` payload. Its JSON bytes come only from `HydrationData::as_str()`, which already escapes script-sensitive characters.

The application remains responsible for projecting public allow-listed fields into JSON and for constructing the initial browser view with coherent initial props/state. Private server data, event callbacks, runtime state, DOM handles, and arbitrary Rust objects are not serialized automatically. The browser API merely parses the optional JSON value; it does not infer which fields are appropriate for a component.

### Validate first, activate second

`DomRenderer::hydrate_into` validates the root markers and recursively matches the client view against the existing SSR DOM before activating listeners or reactive effects. Validation covers the relevant node kinds, initial text, element names, SSR-representable attributes/properties, and nested component/dynamic markers. The hydrator adopts existing nodes; it does not build a duplicate subtree for a compatible initial view.

After validation succeeds, the renderer attaches the view's client event bindings and starts dynamic effects. Delegated events are registered once per mounted boundary/event type using the existing resource model. A successful root marker is changed to `rustflow:root:hydrated:start`; a repeated attempt against that same mount is rejected with `DomError::AlreadyHydrated`.

### Mismatch policy and lifecycle

Missing, duplicated, reversed, or incompatible boundaries and DOM structure return a bounded `DomError::HydrationMismatch` diagnostic. Validation failures do not clear, replace, or mutate the server-rendered root; temporary candidate resources are disposed while preserving the DOM. The caller may show diagnostics through its existing error handling, but must not silently claim that the static fallback is interactive.

This first implementation intentionally uses whole-root static fallback on mismatch. Although ADR 0009 prefers recovery at the smallest safe boundary where that boundary can be classified, safely replacing a subregion requires proven region ownership and state-transfer semantics that this initial path does not yet implement. Do not add destructive partial repair until dedicated tests prove node/state preservation, listener cleanup, and recovery behavior. Non-hydratable component fallbacks remain static.

The same successfully hydrated root cannot be hydrated again. Unmounting a successful mount uses the existing mounted-resource cleanup path for listeners, effects, component instances, and owned scopes.

### Forms, lists, and navigation scope

Hydration starts from the exact server-rendered initial state. A list/conditional region is matched in its initial order/shape; stable view keys are not independently serialized, so the client must reconstruct the same keyed view from public inputs. Form controls are validated against SSR-rendered attributes/value text before listeners are enabled, and validation does not write over user-edited live values.

Client navigation, history integration, link interception, and enhanced form submission are explicitly not part of this ADR; they remain Phase 5.4 work. A link already in the static HTML continues to behave as an ordinary browser link.

## Consequences

Positive:
- Existing SSR pages remain byte-compatible unless callers opt into hydratable output.
- Hydration has deterministic boundary identity and validates before enabling interactivity.
- Existing component/listener/runtime abstractions are reused instead of adding a second registry or runtime.
- Mismatch behavior is explicit and conservative, preserving server output rather than silently rebuilding it.

Limitations:
- The protocol supports only SSR/client views with compatible structural semantics and identical initial state.
- The current mismatch fallback is the whole root, not the smallest nested region.
- Root marker validation detects malformed/duplicate hydration boundaries, but application code remains responsible for choosing the correct client view and initial state.
- This implementation does not enable client navigation or server action/form submission.

## Validation evidence required before closure

- Unit tests verify marker opt-in, deterministic marker sequences, and safe JSON embedding.
- Browser E2E tests verify that a compatible SSR element retains DOM identity and handles repeated clicks once per event, and that a mismatch reports an error while preserving static content.
- CI must pass formatting, Clippy, workspace tests, Rustdoc, WASM compilation, browser E2E and bundle budget, documentation, governance, and dependency review on the final PR head.
- The phase must not be marked complete until the PR is merged to `main` and post-merge CI succeeds.
