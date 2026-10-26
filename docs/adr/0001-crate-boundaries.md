# ADR 0001: Cargo workspace and crate boundaries

- **Status:** Proposed
- **Date:** 2026-10-26
- **Phase:** 0.2 — Product scope

## Context

RustFlow needs independently testable reactivity, view/component, macro, browser, and server functionality. A monolithic crate risks mixing browser-only dependencies with server code and makes incremental testing and feature selection harder. Excessive micro-crates, however, create dependency overhead and unstable boundaries before the APIs are understood.

## Decision proposal

Use a Cargo workspace with a small number of cohesive crates and explicit dependency direction. Begin with the smallest slice required by implementation; split a crate only when a distinct compilation target, dependency set, public contract, or test boundary justifies it.

Candidate responsibilities:
- `rustflow-reactivity`: signals, effects, dependency tracking, scheduling, scopes, and disposal; no DOM or HTTP dependency.
- `rustflow-core` (or equivalent view crate): typed view nodes and component contracts, with no server runtime dependency.
- `rustflow-macros`: procedural macros depending on parsing/code-generation libraries and emitting calls into public core APIs.
- `rustflow-web`: browser DOM and event integration; target-specific dependencies stay here.
- `rustflow-server`: server rendering and HTTP integration behind RustFlow-owned interfaces.
- `rustflow`: optional facade/prelude and feature selection, introduced when a stable composition of crates exists.

Names are provisional until the first workspace manifest is reviewed. Avoid creating empty crates solely to match this list.

## Dependency rules

1. Core reactivity must not depend on browser DOM or server HTTP types.
2. View/core types must not require the server runtime to compile.
3. Browser-only APIs and dependencies must not leak into native/server builds.
4. The facade may depend on public subsystem crates; subsystem crates must not depend back on the facade.
5. Cross-layer behavior is expressed through narrow traits/types owned at the appropriate boundary.
6. Shared infrastructure is reused rather than reimplemented.

## Alternatives considered

- **Single crate:** easiest to start, but risks target/dependency coupling and makes isolated tests harder.
- **One crate per feature immediately:** creates many premature public boundaries and increases maintenance overhead.
- **Workspace with gradual extraction (proposed):** retains testability and target separation while allowing boundaries to evolve with evidence.

## Consequences

Positive:
- Core logic can be tested without a browser or server.
- Native and WASM dependency graphs can be kept separate.
- Public API and feature boundaries are visible.

Costs:
- Workspace and feature configuration require discipline.
- Cross-crate changes may be more verbose.
- Some boundaries will need revision after prototypes.

## Validation before acceptance

- Build a minimal workspace prototype with reactivity tests.
- Check native and WASM dependency separation.
- Review package names, MSRV declaration, feature defaults, and dependency policy.
- Accept or amend this ADR before treating the crate split as final.
