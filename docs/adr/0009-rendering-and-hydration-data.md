# ADR 0009: Server/client rendering and hydration-data contract

- **Status:** Proposed for review
- **Date:** 2026-10-27
- **Phase:** 5.1 — Rendering contract

## Context

RustFlow has a target-independent structural view model, a reactive browser DOM adapter, and an SSR serializer. The SSR crate currently escapes markup and rejects unsafe elements/attributes, but it has no hydration-data API or executable component-capability marker. A separate crate boundary reduces coupling but does not prove that the browser dependency graph excludes server crates, nor can a generic serializer infer whether application data contains secrets.

Phase 5.1 must define a narrow, testable boundary without implementing the full hydration protocol, route-data composition, or client navigation.

## Decision

### Component capabilities and environment rules

Define three conceptual capabilities for the forthcoming component API:

- **Universal:** setup/render may execute in both environments; it depends only on target-independent RustFlow APIs, receives explicit input, and produces a structural view without browser/server side effects during render.
- **Server-only:** may access request-scoped server services and private data, but is never included in or invoked by a browser/WASM component graph.
- **Browser-only:** may access DOM, browser events, browser storage, or browser scheduling. SSR must not evaluate it as if those APIs existed on the server; it needs an explicit static/non-hydratable fallback or a deliberate client-only boundary.

The categories are represented by `ComponentCapability` on `ComponentDefinition`. Definitions default to `Universal` for compatibility, while `.server_only()`, `.browser_only()`, and `.with_capability(...)` allow the author to narrow execution. `ComponentRegistry::mount_for_target` checks the declared target before setup; target-neutral mount APIs accept universal components only. The SSR renderer skips browser-only setup and serializes the explicitly configured fallback under the normal HTML safety policy, or returns `SsrError::NonHydratableComponent` if no fallback exists. The browser DOM renderer rejects server-only components before setup. Do not disguise browser-only access as a universal component or infer universal compatibility from successful native compilation alone.

These are runtime execution guards, not compile-time proof that an application has omitted server-only code from a WASM binary. Crate boundaries and the WASM dependency-graph check remain required; applications must not register private server components in browser registries.

Components and render functions must use explicit props/context and owner-scoped runtime state. Server rendering creates isolated runtime state per render/request; no process-global mutable runtime, hidden request singleton, or cross-request component state is allowed. Render evaluation must be repeatable and must not install persistent effects or perform externally visible side effects.

### Hydration data format and trust boundary

Choose **JSON values** as the initial logical data model, represented by `serde_json::Value`. It is broadly interoperable with JavaScript, has a familiar type system, and avoids introducing a second serialization framework or a custom binary decoder in the browser. Do not serialize arbitrary Rust structs through a blanket `Serialize` API at this boundary. Application code must explicitly project public fields into a JSON value.

The new `rustflow_ssr::HydrationData` API provides the following minimum checks:

- Accept only JSON `null`, booleans, numbers, strings, arrays, and objects.
- Reject malformed JSON, inputs larger than 1 MiB, nested values deeper than 64 levels, and final escaped output larger than 1 MiB.
- Produce compact JSON and encode `<`, `>`, `&`, U+2028, and U+2029 as JSON Unicode escapes. This prevents HTML script-text delimiters from appearing literally in this data string while preserving valid JSON after parsing.
- Return explicit errors without echoing payload contents.
- Keep the data string distinct from HTML: this type does not create a `script` element, does not authorize raw HTML insertion, and does not implement client hydration.

The byte and depth limits are an initial documented contract and may be changed only with tests and a reviewed rationale. JSON is preferred over raw embedded JSON because raw `</script>` can terminate an HTML script element; MessagePack/postcard or a bespoke format are deferred because they add a decoder/dependency and interoperability burden without a current measured need.

### Public data versus private server values

Only explicitly selected client-visible fields may cross the hydration boundary. Never include credentials, session cookies/tokens, request objects, authorization decisions, database credentials, private server state, closures, event callbacks, DOM handles, component/runtime IDs, or arbitrary internal objects. Server secrets must remain in server-only services and must not be serialized or captured by browser component code.

Rust types do not inherently encode whether a string/value is secret. `HydrationData` can validate the JSON representation but cannot discover that a caller deliberately included a secret. Therefore, a public DTO/allow-list at the application boundary is mandatory; tests validate the serializer's observable properties, not the semantic sensitivity of application values.

### Client dependency isolation

The existing browser example depends on `rustflow`, `rustflow-view`, and `rustflow-dom`; it does not need `rustflow-server` or `rustflow-ssr`. Add a CI check over `cargo tree -p rustflow-browser --target wasm32-unknown-unknown --edges normal` to reject the currently identified server-only crates and runtime dependencies (`rustflow-server`, `rustflow-ssr`, Axum, Tokio, Tower, and Rustls). Keep the deny-list synchronized when the architecture adds or renames server-only dependencies.

This check validates the known dependency boundary; it does not prove that arbitrary application values contain no secrets, does not inspect every possible generated artifact, and is not a substitute for checking the actual production bundle. The existing WASM compilation and browser bundle/E2E jobs remain required.

### Determinism, hydratability, and fallbacks

A render is deterministic only when the structural tree, component registry/definitions, explicit props, public route data, feature configuration, and iteration order are the same, and component rendering avoids clocks, randomness, environment-dependent locale/formatting, process-global mutable state, and uncontrolled side effects. Keyed children must use stable logical keys. A deterministic static-tree regression test is added; this does not promise determinism for arbitrary user closures or browser state.

A fragment is hydratable only when the server markup and initial browser render are derived from the same supported component definition, explicit public inputs, stable keys/order, and compatible structural semantics. Event callbacks, closures, owner/runtime references, and DOM objects are never part of the serialized payload.

Until a real hydration implementation and boundary metadata exist, non-hydratable content must not be silently treated as hydrated. The eventual fallback policy is:
1. Preserve server-rendered static markup for an explicitly static/non-hydratable boundary and do not claim it has client interactivity; client replacement must be an explicit choice.
2. Validate a candidate boundary before attaching listeners or mutating live DOM.
3. On mismatch, report a bounded diagnostic and recover only within the smallest known boundary; do not attach duplicate listeners, silently accept unsafe markup, or discard compatible component state outside that boundary.
4. If a boundary cannot be classified safely, leave it static and report the limitation rather than guessing.

These are target rules for later implementation. The current code does not yet detect hydration mismatches, mark boundary identities, preserve DOM during hydration, or implement fallback recovery.

## Alternatives considered

- **Raw JSON inside HTML:** rejected because HTML script parsing can treat a literal closing script delimiter as the end of the data element.
- **Serialize arbitrary Rust values automatically:** rejected because type serializability does not mean the value is safe or public, and it encourages accidental leakage of private fields.
- **Custom binary format:** deferred; it needs a browser decoder and offers no proven advantage at the current scope.
- **Runtime-only component markers:** flexible, but can fail late if a browser-only component is invoked on the server.
- **Separate server/client component traits or generated compile-time markers immediately:** potentially stronger, but current `ComponentDefinition`/registry APIs do not express route, render, or environment capabilities. Defer the exact public API until the SSR composition work in 5.2 clarifies its concrete consumers; do not claim enforcement before the API and negative compile/runtime tests exist.

## Consequences and remaining work

Positive:
- Hydration data has a bounded JSON representation and an observable escaping/validation contract.
- CI checks the currently known server dependency boundary for the browser target.
- Component and mismatch semantics are documented before SSR/hydration integration.

Costs and limitations:
- Component capabilities are runtime metadata enforced by target-aware mount/render paths; they are not a proof that user-authored server-only code is absent from every possible binary.
- Secret exclusion depends on explicit public-data projection at the application boundary; the framework cannot semantically detect arbitrary secrets in JSON. This is the documented trust boundary, not a promise of automatic secret detection.
- Determinism is guaranteed only for constrained static inputs; arbitrary component render closures remain outside that guarantee.
- The fallback is static markup, not a hydration boundary. Mismatch detection, partial DOM recovery, and listener attachment are not implemented.
- Full SSR composition, route data, hydration IDs, event attachment, mismatch detection/recovery, navigation, and forms remain future phases.

## Validation

- Unit tests cover script-context escaping, JSON round-tripping, malformed/oversized input, excessive nesting, output-size limits, and repeatable compact encoding.
- A static-tree SSR test checks identical HTML bytes for repeated renders.
- Target-aware component tests verify server-only definitions are rejected in browser rendering and browser-only definitions are rejected in server mounting before setup; SSR tests cover both static fallback and missing-fallback errors.
- CI checks the known server-only dependency list is absent from the browser/WASM graph.
- The Rust toolchain, Clippy, full workspace tests, Rustdoc, WASM build, browser E2E/bundle budget, documentation-site build, governance, and dependency review must pass on the PR before this ADR/subphase is treated as verified.
