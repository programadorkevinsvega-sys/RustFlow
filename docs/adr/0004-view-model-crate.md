# ADR 0004: Isolate the structural view model

- **Status:** Accepted
- **Date:** 2026-10-26
- **Phase:** 2.1 — View representation

## Context

Phase 1 provides a native, single-threaded reactive runtime in `rustflow`. Phase 2 needs a target-independent view representation that can be tested without a browser and does not bring DOM or server dependencies into the core model. The earlier crate-boundary ADR deliberately left exact names and boundaries provisional.

## Decision

Introduce `rustflow-view` as the owner of the structural view model. The `rustflow` crate re-exports it at `rustflow::view`, so consumers have one entry point while the view types remain independently testable.

The dependency direction is `rustflow -> rustflow-view`. The view crate must not depend on the facade, the reactive runtime, browser APIs, or server infrastructure.

The first implementation contains text, element, and fragment nodes. Component nodes, attributes, properties, boolean attributes, events, dynamic children, keys, construction errors, and rendering contracts remain separate roadmap items. They will be added when their behavior and lifecycle semantics are defined and covered by tests.

The crate is named `rustflow-view`, not `rustflow-vdom`, because this decision does not commit the project to a virtual-DOM diffing algorithm. The client-runtime ADR prefers targeted DOM updates, subject to implementation evidence.

## Alternatives considered

- **Keep the model inside `rustflow`:** fewer packages initially, but mixes the facade/reactivity implementation with an independently testable public view contract.
- **Create a general `rustflow-core` immediately:** a broader boundary than the current implementation needs and could collect unrelated responsibilities.
- **Create `rustflow-vdom`:** implies a virtual-DOM strategy that has not been selected and may conflict with targeted updates.

## Consequences

- Structural view types compile and test without a browser or server.
- The public facade remains the normal entry point.
- There are no new third-party dependencies.
- The new API is a prototype, not a rendering API or a production-ready component system.
- Future component, renderer, and lifecycle work must preserve this dependency direction or document a reviewed architectural change.

## Validation

- Unit tests cover verbatim text storage, element tag/child ordering, fragment grouping, and empty child collections.
- Workspace CI must verify the MSRV and stable toolchains, formatting, Clippy, tests, and documentation builds before this change is considered validated.
