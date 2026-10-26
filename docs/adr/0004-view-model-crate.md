# ADR 0004: Isolate the structural view model

- **Status:** Accepted
- **Date:** 2026-10-26
- **Phase:** 2.1 — View representation

## Context

Phase 1 provides a native, single-threaded reactive runtime in `rustflow`. Phase 2 needs a target-independent view representation that can be tested without a browser and does not bring DOM or server dependencies into the core model. The initial crate-boundary decision kept names and boundaries provisional until an implementation demonstrated the correct seam.

## Decision

Use `rustflow-view` as the owner of typed, target-independent view data. The `rustflow` facade re-exports it at `rustflow::view`. The workspace keeps the responsibilities separated:

- `rustflow-view`: text, element, fragment, component descriptor, dynamic child and keyed-list nodes; typed attributes, properties and events; syntax validation and construction errors; and the renderer contract.
- `rustflow`: reactivity, component definitions, component registry resolution, component instances and lifecycle/cleanup.
- `rustflow-macros`: the constrained `view!` procedural macro. It emits calls to the public view model rather than embedding runtime behavior.

The dependency direction is `rustflow -> rustflow-view`, and `rustflow -> rustflow-macros`. The view crate does not depend on the facade, reactive runtime, browser APIs, or server infrastructure. The macro crate emits code that calls the facade's documented public path.

The typed view model validates the syntax of element, attribute, property, event and component names. This is not an HTML allowlist, URL sanitizer, or rendered-output security boundary. Text and attribute values must be escaped by any future renderer, URL-bearing attributes need an explicit policy, and raw HTML must not be inserted implicitly.

The crate is named `rustflow-view`, not `rustflow-vdom`, because the project has not committed to a virtual-DOM diffing algorithm. The client-runtime ADR favors targeted DOM updates, subject to implementation evidence.

## Alternatives considered

- **Keep the model inside `rustflow`:** fewer packages initially, but mixes the facade/reactivity implementation with an independently testable view contract.
- **Create a general `rustflow-core` immediately:** broader than the current need and risks collecting unrelated responsibilities.
- **Create `rustflow-vdom`:** would imply a virtual-DOM strategy not selected by the current renderer contract.

## Consequences

- View types compile and test without a browser or server.
- The public facade remains the usual entry point.
- No third-party dependency was added for the model or macro parser.
- Component descriptors can be resolved to reusable definitions through the native registry; recursive tree traversal and reconciliation remain renderer responsibilities.
- The API remains pre-1.0 and is not a production-ready web framework or a safe-HTML representation.
- Future component, renderer, and lifecycle work must preserve the dependency direction or document a reviewed architectural change.

## Validation

The Phase 2 implementation includes tests for syntax validation, typed attributes/properties/events, dynamic children, keyed lists, component definition/registry/instance lifecycle, macro expansion and compile-fail diagnostics. Final workspace validation is recorded by the GitHub Actions checks attached to the implementing pull request.
