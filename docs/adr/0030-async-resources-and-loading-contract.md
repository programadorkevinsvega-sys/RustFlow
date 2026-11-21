# ADR 0030: Asynchronous resources and loading contract

- **Status:** Accepted for implementation
- **Date:** 2026-11-21
- **Phase:** 2.5 - asynchronous resources
- **Scope:** Owner-scoped async state, cancellation, stale requests, SSR snapshots, and loading UI

## Context

RustFlow's reactive runtime is eager and single-threaded. It uses `Rc` and
`RefCell`, tracks signal dependencies synchronously, and owns cleanup in
component scopes. Existing effects do not track dependencies across `await`, and
the runtime does not provide a hidden async executor.

Resource support must preserve those boundaries instead of introducing a
process-global task registry or positional hook machinery.

## Decision

### Resource API

- Implement `Resource<T, E>` and `ResourceRequest<T, E>` as owner-scoped handles.
- Create resources during component setup with `Scope::resource`,
  `try_use_resource`, or `use_resource`.
- A resource exposes `Idle`, `Loading`, `Reloading`, `Ready`, `Error`, and
  `Cancelled` states.
- `begin()` starts a new request. `complete(value)`, `fail(error)`, and
  `cancel()` explicitly settle it.
- Dropping an unsettled `ResourceRequest` cancels its current request.

### Cancellation and stale results

- Every request receives a monotonically increasing request identifier.
- A new request invalidates older request handles.
- Completions and failures from stale handles report `false` and do not change
  state.
- Cancellation increments the invalidation state and exposes `Cancelled`.
- Owner disposal invalidates the resource and all request handles.
- Cleanup is owner-bound and LIFO through the existing owner cleanup contract.

### No automatic async tracking

- Resources do not poll Rust `Future` values internally.
- Resources do not start tasks, Tokio jobs, browser tasks, or a hidden executor.
- Applications may bridge their existing executor by obtaining a request handle,
  then explicitly completing or failing it.
- This keeps async work deterministic and avoids claiming dependency tracking
  across `await`.
- The resource type remains `!Send`/`!Sync` like the reactive runtime.

### Reloading and loading UI

- `Loading` means no previous ready value exists.
- `Reloading` preserves the previous `T` as stale data while a later request
  runs.
- RustFlow does not add Suspense or a loading boundary in this phase.
- Components match `ResourceState` explicitly and decide whether to preserve
  stale UI or present a loading fallback.
- One owner scope may own multiple resources; no global async state is added.

### SSR and hydration

- SSR does not execute or wait for client resources implicitly.
- SSR may install a public snapshot with `hydrate_ready`, `hydrate_error`, or
  `hydrate`.
- A resource snapshot is an explicit application decision. Callers must project
  only public, serialized fields and must not place sessions, credentials,
  cookies, authorization data, or private server state in it.
- Hydration starts in `Loading` unless the application supplies a snapshot.
- Server-provided snapshot mismatches with the browser result are handled as a
  normal stale/current request race: newer browser results win only when the
  browser request is still current.
- The existing bounded hydration JSON protocol remains responsible for transport
  limits and escaping.

## Consequences

- Deterministic races and cleanup are testable without wall-clock timing or an
  external runtime.
- Applications remain responsible for task ownership and for choosing a browser
  or server executor.
- Reloading UI can use stale data deliberately; stale completions cannot replace
  a later state.
- Suspense, automatic prefetch, and a fetch-tracking component boundary are not
  exposed in this phase.
- A resource handle is invalid after owner disposal and must not be used to revive
  unmounted UI.

## Verification

- Unit tests cover idle/loading/reloading/ready/error/cancelled states.
- Unit tests cover stale completion, stale failure, explicit cancellation,
  request handle drop, hydration snapshots, and owner disposal.
- Public docs state the single-threaded boundary and absence of Suspense.
- Browser, SSR, and hydration integration remain an explicit follow-up only when
  a renderer adopts resource state snapshots.
