# ADR 0002: Client runtime and rendering model

- **Status:** Proposed
- **Date:** 2026-10-26
- **Phase:** 0.2 — Product scope

## Context

RustFlow's differentiator is a Rust-owned reactive UI model that targets the browser through WebAssembly. The runtime must define how state changes reach the DOM, how event listeners are owned, and how components release resources. A broad virtual-DOM implementation or hydration system before basic reactivity is proven would add complexity before the core contracts are known.

## Decision proposal

Start with a small client runtime compiled to `wasm32-unknown-unknown`, using `wasm-bindgen` and `web-sys` where appropriate. Keep the reactive graph and view contracts testable on native targets when possible. Begin with targeted DOM updates driven by RustFlow's own dependency tracking rather than committing to a full virtual-DOM diffing algorithm.

The first supported rendering surface is intentionally constrained:
- text nodes, elements, fragments, and nested components;
- a documented initial set of attributes/properties and event handlers;
- conditional rendering and keyed dynamic lists;
- explicit component ownership and deterministic listener/effect cleanup.

Use semantic HTML and accessible form controls. Server rendering and hydration are out of scope for the first interactive browser prototype.

## Alternatives considered

- **Full virtual DOM first:** offers a familiar diffing model but may add allocation and diff complexity before performance needs are measured.
- **Direct DOM updates from reactive dependencies (proposed):** aligns with targeted reactivity and supports a narrow vertical slice; requires careful node identity and lifecycle contracts.
- **JavaScript framework wrapper:** could accelerate UI breadth but conflicts with the goal of a RustFlow-owned reactive runtime.
- **Server-rendered HTML and hydration from day one:** potentially improves initial content delivery but substantially expands serialization, determinism, and mismatch-recovery requirements.

## Consequences

Positive:
- The MVP can prove the key reactivity-to-DOM path early.
- DOM changes can be targeted to observed dependencies.
- Lifecycle cleanup is part of the design rather than an afterthought.

Costs:
- Direct DOM ownership and keyed-list identity require precise contracts.
- WebAssembly tooling adds build and debugging complexity.
- Browser-only code needs separate target checks and browser tests.

## Validation before acceptance

- Prototype a counter and todo list.
- Verify that unrelated effects do not update unrelated DOM nodes.
- Test listener/effect cleanup on unmount and keyed list identity.
- Run browser tests and record bundle-size methodology before making performance claims.
- Revisit the rendering strategy if profiling or implementation evidence contradicts the proposal.
