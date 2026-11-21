# ADR 0031: Component ErrorBoundary contract

- **Status:** Accepted for implementation
- **Date:** 2026-11-21
- **Phase:** 2.6 - UI error boundaries
- **Scope:** Component setup/render UI failures, SSR fallback, browser isolation,
  cleanup, nesting, and recovery

## Context

RustFlow currently exposes reactive DOM error state but does not provide a
component-level fallback boundary. Component setup/render failures propagate
through the component host and SSR can return no usable partial UI.

An ErrorBoundary must isolate only UI/component failures. It must not become a
catch-all for HTTP, authentication, authorization, database, or server-action
errors, and it must not expose raw errors or secrets in fallback HTML.

## Decision

### Public shape

- Add `View::error_boundary(child, fallback)`.
- It creates the reserved `ErrorBoundary` component descriptor.
- The descriptor has exactly two children:
  1. the protected child view;
  2. the accessible fallback view.
- A malformed child count is a typed view/component error.

### Captured failures

An ErrorBoundary captures:

- component setup/render `ComponentError`;
- view-construction and view-adapter errors below the boundary;
- renderer panics in the protected subtree, converted to a fallback result;
- nested boundary failures handled by the nearest boundary.

An ErrorBoundary does not capture or reinterpret:

- HTTP routing or server infrastructure failures;
- authentication, authorization, CSRF, or server-action responses;
- resource request state after it returns `Error`;
- user event callback failures outside the mounted component tree.

Those server and resource errors remain explicit application/server contracts.

### Cleanup and recovery

- Entering fallback must dispose the protected component scope and its listeners,
  effects, resource handles, and descendant scopes.
- The fallback view must not inherit hidden server state or dynamic component
  resources.
- If a later boundary update renders a protected view successfully, the protected
  child is mounted again and the fallback range is replaced.
- If a protected child fails again, the boundary returns or retains its safe
  fallback.
- Repeated disposal is harmless.

### SSR and hydration

- SSR may render the fallback when the protected subtree fails locally.
- SSR must not emit raw panic text or server error details.
- The browser boundary may recover after a valid re-render or property update.
- Hydration mismatch policy remains conservative: a mismatch is not repaired by
  bypassing the DOM validation protocol.

### Accessibility and safety

- Fallback output is an ordinary escaped `View`; raw HTML is never introduced.
- Applications should use semantic status markup and avoid role-only or text-only
  meaning.
- Fallback copy must not include secrets, token values, stack traces, SQL text,
  request bodies, or serialized server state.

## Alternatives considered

1. **Boundary inside `View::Element` only:** rejected. It would not cover
   component setup/render lifecycle or nested component scopes.
2. **Global panic hook:** rejected. It is process-global, not owner-bound, and
   cannot distinguish nested boundaries or server errors.
3. **Catch-all for HTTP/server responses:** rejected. HTTP infrastructure belongs
   to routing/server adapters and must keep transport status/authorization semantics.
4. **Automatic retry:** rejected. Recovery must be explicit after a render/update
   is known to succeed.

## Consequences

- UI isolation is deterministic and owner-bound.
- The boundary has one explicit shape and no hidden fallback registry.
- SSR and browser behavior must stay aligned with the same two-child contract.
- Component-level fallback is available without introducing hook state or a
  process-global error store.
- Server failures remain visible to server logging/HTTP policies and are not
  silently rewritten as successful UI.
