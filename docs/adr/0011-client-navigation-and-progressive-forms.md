# ADR 0011: Client navigation and progressive form enhancement

- **Status:** Proposed for review; implementation merged in PR #41, formal architectural approval remains pending
- **Date:** 2026-10-27
- **Phase:** 5.4 — Navigation and forms
- **Related:** ADR 0009, ADR 0010, `docs/SSR_RENDERING.md`

## Context

At the time this ADR was proposed, RustFlow had an HTTP server prototype, full-document SSR helpers, opt-in DOM hydration, and a reactive DOM mount but no client navigation adapter. Phase 5.4 has since implemented the adapter described below. RustFlow still does not provide a framework-owned general client route table or server actions.

The browser must keep ordinary HTML navigation and form submission as the baseline. Client enhancements cannot turn a missing route, failed server request, or incompatible response into a false success, and they must not destructively replace valid SSR content during hydration.

## Considered options

1. **Intercept every same-origin navigation and form.** Rejected: this would silently change POST, target, download, fragment, and other browser semantics, and would create a second implicit route system.
2. **Add a general-purpose client router and server-action framework.** Rejected: a route DSL, data cache, action protocol, session/authentication support, and mutation lifecycle exceed this phase.
3. **Provide an explicit opt-in adapter around the existing reactive mount, with an application-supplied route callback.** Chosen: it can validate the HTTP response, update the existing reactive view, reuse DOM ownership/cleanup, and leave unsupported interactions native.

## Decision

### Client navigation API and route ownership

The browser-only `rustflow_dom::ClientNavigation::install(target, mounted, on_navigate)` API owns the existing `ReactiveMountedDom` and installs delegated click/submit listeners plus one `popstate` listener. It must only be installed after an initial `mount_reactive` or successful `hydrate_reactive` call. Repeated installation without dropping the previous handle is an application error; dropping the handle removes its listeners and disposes the mount.

The application callback receives a path plus query string (for example, `/articles?page=2`). It returns `Ok(true)` only after synchronously updating the application's route state for a route the client supports. It returns `Ok(false)` for an unsupported route without changing state. Route definitions, public data projection, and view construction remain application-owned; this API is not a router DSL and does not deserialize server state.

### Eligible links

Intercept a click only when all of the following are true:

- It is an unmodified primary-button click on a link inside the supplied mount target.
- The destination uses HTTP or HTTPS and has the same origin.
- The destination has no fragment, is not the current URL, and has no `download` attribute or `rel="external"`.
- The link has no `data-rustflow-no-nav` opt-out and its target is empty or `_self`.

External links, other origins/protocols, downloads, explicit alternate targets, modified clicks, fragment navigation, and ineligible links keep native browser behavior. Interception is delegated and scoped by containment; it does not globally cancel navigation.

### Request, view and history commit

Before changing the visible route or history, RustFlow performs a same-origin GET request and requires a successful HTTP status, a `text/html` content type, a complete HTML document, and a final same-origin URL after redirects. The app callback must then accept the destination and the existing reactive mount must complete its update without reporting a render error. Only then does an ordinary client transition call `history.pushState`. A successful `popstate` transition leaves the selected browser history entry in place, replacing its URL only when the request redirected to another same-origin URL.

The response body is validated as a full HTML document, not injected into the existing DOM. The application reconstructs its supported view through the supplied route callback and existing RustFlow view/runtime. This is deliberate: RustFlow never inserts response HTML with `innerHTML`, never serializes arbitrary server state, and never pretends an unknown client route was rendered.

If the request fails, returns a non-success status, is not HTML/a complete document, redirects outside the origin, the callback rejects the route, the route render reports an error, or the History API operation fails, the candidate view is restored to the previous route where possible and the browser performs a full navigation (or reloads the selected URL on `popstate`). Network/HTTP failures are logged as diagnostics but are not exposed as successful client transitions. The server remains authoritative for 404/405/error responses.

Concurrent requests use a generation counter so a stale response cannot overwrite a newer transition. Listener closures and the reactive mount share one owner; dropping `ClientNavigation` detaches listeners and releases the mount, allowing the existing DOM runtime to clean up effects, components and listeners.

### Progressive form enhancement

Only forms explicitly marked `data-rustflow-enhance` are candidates. The supported subset is same-origin HTTP(S), `GET`, `application/x-www-form-urlencoded`, empty/`_self` target, no fragment, a valid form, and scalar input/select/textarea controls whose submission semantics are supported by the adapter.

POST/PUT/PATCH/DELETE, file and image inputs, multiple selects, named submit controls, explicit alternate targets, opt-out forms, unsupported encodings, invalid forms, and any form the adapter cannot serialize exactly stay on the native HTML submission path. The enhanced GET request preserves existing query parameters and appends successful scalar controls; disabled controls and unchecked checkbox/radio inputs are skipped. A temporary pending attribute prevents duplicate enhanced submissions and is cleared after success or failure. A failed request falls back to a full form navigation, so HTTP errors are handled by the server rather than rendered as success.

The contract deliberately excludes server actions, upload infrastructure, CSRF, authentication, sessions, and private server state. Applications must not encode secrets into GET form parameters or hydration payloads.

### SSR, hydration and no-JavaScript guarantees

- Ordinary SSR output remains meaningful HTML independently of the browser bundle.
- Without JavaScript, before client initialization, or when an interaction is ineligible/unsupported, normal browser navigation and HTML form submission remain available.
- Hydration remains opt-in. The router does not start before the app has a successfully mounted/hydrated reactive view. A hydration mismatch keeps the existing SSR root static per ADR 0010; the caller must not install client navigation after failed hydration.
- The browser history URL is updated only after a supported route has rendered. Back/forward and failure paths either settle on the supported view or trigger a full server navigation/reload.
- No arbitrary server response markup, event handler, or private state is injected into the running view.

## Consequences and limitations

This keeps the public API small and reuses existing DOM/reactivity ownership. In exchange, applications must provide their route callback and view state; the current API does not parse SSR response content into a RustFlow view, offer route loaders, or provide a client route table. Its HTML compatibility check establishes that the response is a successful same-origin full document; application route acceptance and view/render errors remain necessary checks. The supported enhanced form subset is intentionally narrower than the full HTML form specification, and unsupported cases stay native.

## Validation required before acceptance

- Unit and E2E tests must prove eligible client navigation, URL/history/back/forward, direct route and reload behavior, and fallback for failed/incompatible server responses.
- E2E tests must prove native behavior for modified/external/alternate-target/download links; opted-in supported GET forms; duplicate-submit suppression; and native submission for non-opted-in/unsupported forms.
- Existing hydration mismatch-preservation and lifecycle-cleanup tests must remain passing.
- **Implementation verification:** PR #41 was merged into main as `1da8b110ed3e57abccb2c14b6236fcbbc0fbf849`. The PR-head CI passed Rust 1.85/stable, WASM, browser build, bundle budget, Playwright E2E (18 tests), docs build, governance and dependency review. The post-merge CI visible on `cc26782ef28cf14c5ad039bdcbdded93a5972a34` passed Rust 1.85/stable, WASM, browser E2E/bundle size, Cloudflare Pages and governance. The implementation/phase status is therefore complete within the documented scope. The ADR remains proposed until maintainers complete its formal decision review; proposal status must not be interpreted as code being absent.
