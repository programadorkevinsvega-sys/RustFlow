# HTTP server foundation (prototype)

The `rustflow-server` crate is the HTTP adapter and composition boundary. It wraps Axum, Tokio, and Tower behind RustFlow-owned request, response, error, configuration, and route types. It is a pre-1.0 foundation, not a production deployment recipe.

## Ejecutar el ejemplo

Ejecuta `cargo run -p rustflow-server --example basic`. El servidor de ejemplo escucha en `127.0.0.1:3000` y expone `/`, `/api/health`, `/users/{id}?detailed=true` y `POST /api/echo`. Añade una cabecera mediante middleware global, agrupa las rutas `/api/*` con el prefijo de nombre `api`, imprime la tabla de rutas y usa `Ctrl+C` como future de apagado graceful.

## Route example

```rust
use rustflow_server::{Application, Request, Response};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let mut app = Application::new();
app.get("/users/{id}", |request: Request| async move {
    let id = request.path_param("id").unwrap_or("unknown");
    Ok(Response::text(format!("user {id}")))
})?;
# Ok(())
# }
```

Route groups share a prefix and optional middleware:

```rust
use rustflow_server::{Application, Middleware, Request, Response};

# fn example(app: &mut Application) -> Result<(), rustflow_server::ServerError> {
app.group("/api", |api| {
    api.middleware(Middleware::new(|request, next| async move {
        next.run(request).await
    }));
    api.get("/health", |_request: Request| async { Ok(Response::text("ok")) })?;
    Ok(())
})?;
# Ok(())
# }
```

Handlers are async closures that return `Result<Response, HttpError>`. Route handlers can capture shared state explicitly; this prototype intentionally has no hidden global service locator. Path parameters are decoded from the route template. Query strings can be decoded with `Request::query::<T>()`, and JSON bodies with `Request::json::<T>()`. Phase 6.3 adds `Request::form::<T>()` for URL-encoded bodies, `validated_form`, `validated_json`, and `validated_query` with the `ValidateInput` trait, and structured HTTP 422 field/form errors. Malformed input returns a generic 400; an unsupported form media type returns 415.

## Responses

Use `Response::text`, `Response::html`, `Response::json`, `Response::redirect`, `Response::file_download`, or `Response::status`. `Response::html` emits the supplied string as-is, so it must only receive trusted/escaped content. Prefer `html_view` for structural fragments without components. For a complete document use `html_document(title, lang, &view)`; for local callbacks the helper is `html_document_with_components`, while concurrent HTTP handlers should use `html_document_with_shared_components` with a startup-built `SharedComponentCatalog`.

## Sessions, identity, and authorization

The experimental Phase 6.4 API provides SessionManager, SessionContext and the SessionStore contract. The default cookie is host-only __Host-rustflow_session with Secure, HttpOnly, SameSite=Lax, Path=/, no Domain, and Max-Age derived from server-side expiry. Plain HTTP requires the explicit SessionCookieConfig::development() policy, which must never be used in production.

Install sessions.middleware() before guards. Use route_protected or route_authorized to check access before the handler. A missing identity returns 401, an authorization denial returns 403, and storage/policy failures return a generic 500. Missing policies and permission rules deny by default. Identity and roles are read from server-side session records only.

InMemorySessionStore is volatile, process-local, and does not coordinate instances. A durable adapter and database transaction/migration policy belong to Phase 6.5. The application must verify credentials before calling SessionContext::authenticate. OAuth/OIDC, MFA, account recovery, rate limiting and user-provider integration are not implemented. Sessions do not replace CSRF: keep CsrfProtection on cookie-authenticated mutating routes, including login/logout. See the public sessions guide and ADR 0015.

## Resource limits

Defaults cap buffered request bodies at 1 MiB, allow up to 256 concurrent requests, and time out body reads/handlers after 30 seconds. These are initial defaults, not a substitute for reverse-proxy, TLS, deployment, rate-limit, or application-specific protections. Use `Application::with_config(ServerConfig::new()...)` to change them.

## Middleware, route groups, and graceful shutdown

Register global middleware with `app.middleware(Middleware::new(|request, next| async move { /* before */ let response = next.run(request).await?; /* after */ Ok(response) }))`. The stack is ordered; middleware may short-circuit without calling `next.run`.

Scope routes with `app.group("/api", |api| { api.middleware(...); api.get("/health", handler)?; Ok(()) })`. Nested groups inherit the parent prefix and middleware. `name_prefix("api")` joins named routes as `api.health`. Attach extra middleware to one registered method/path with `route_middleware(HttpMethod::Get, "/open", middleware)`. Execution order is global, then group, then route-scoped.

Register a route with `route_named("users.show", HttpMethod::Get, "/users/{id}", handler)` and inspect it with `app.routes()`.

For shutdown, call `serve_with_shutdown(address, shutdown_future)` or `serve_listener_with_shutdown(listener, shutdown_future)`. The application composition root owns the trigger (for example, an OS signal future); the adapter does not silently install a process-wide signal handler.

## TLS, trusted proxies, and production wiring

`ServerConfig::production()` requires HTTPS after client-address resolution. Plaintext `serve` is accepted in production only when `with_trusted_proxies` lists the reverse-proxy networks that may set `Forwarded` / `X-Forwarded-For` / `X-Forwarded-Proto`. Direct client addresses with HTTPS use `serve_tls` / `serve_tls_with_shutdown` and PEM material via `tls_server_config_from_pem`. Call `validate()` before a plaintext bind and `validate_tls()` before a TLS bind.

Forwarded headers are ignored unless the TCP peer is in the trusted-proxy set. `Request::client_ip()` and `Request::is_https()` expose the reconstructed identity. For RFC 7239 `Forwarded`, the scheme is taken only from the rightmost element; an absent, invalid, or duplicated `proto` there fails closed instead of falling back to an earlier element that could be client-supplied. For comma-separated `X-Forwarded-Proto`, the rightmost token is used. Configure the trusted edge proxy to emit the authoritative scheme value at the rightmost position and protect the proxy-to-origin connection. Spoofed values from untrusted peers do not change the resolved identity. Missing HTTPS when required returns 400 with a generic public message.

The TLS listener accepts TCP connections independently of their TLS handshakes. Handshakes run concurrently with a hard cap of 64 and a 10-second timeout; excess handshakes are dropped rather than allowed to create an unbounded queue. Dropping the server listener cancels the accept loop and pending handshakes. This is an availability guard, not a substitute for a production TLS/proxy deployment review.

## Request IDs and tracing

Every handled request receives a correlation ID exposed as `Request::request_id()` and echoed in the response header `x-request-id`, including body-read errors. A supplied ID is retained only if it is 1–128 ASCII letters, digits, dots, underscores, or hyphens; otherwise the server generates an ID with the `rf-` prefix. It is for log correlation only and must never be used as an authorization credential.

The adapter emits a `tracing` span named `http.request` with the request ID, HTTP method, and registered route template. It records completion/failure status through tracing events. The library does not install a global subscriber: configure `tracing-subscriber` (or an application-selected tracing layer/exporter) in the binary/composition root.

## Complete documents and configurable error pages

Use `html_document(title, lang, &view)` when a handler should return a complete HTML document rather than a fragment. It builds the document shell, UTF-8 charset metadata, title, and body structurally. The title, language attribute, and view text are escaped/validated by `rustflow-ssr`. For concurrent routes, build a `SharedComponentCatalog` at application startup and use `html_document_with_shared_components` or `HtmlPage::with_shared_components`. The local `html_document_with_components` remains available for non-shareable callbacks. See `docs/SSR_RENDERING.md` and the runnable server example for the preferred pattern.

Configure pages for selected error statuses through `ErrorPages`:

```rust
use http::StatusCode;
use rustflow_server::{Application, ErrorPages, ServerConfig};

let error_pages = ErrorPages::new()
    .with_page(StatusCode::NOT_FOUND, "No encontrado", "La página no existe.")
    .expect("404 is an error status");
let config = ServerConfig::new().with_error_pages(error_pages);
let app = Application::with_config(config);
```

Only 4xx/5xx statuses can be configured. Unconfigured statuses keep their normal response. Configured title/message strings are rendered as text nodes inside a full HTML document, never as raw HTML. The adapter preserves the original status and `x-request-id` header when replacing the body, including router-generated 404/405 responses.

## Boundaries and limitations

Routing supports conventional methods and Axum path templates such as `/users/{id}`. Duplicate method/path registration and equivalent parameter-only route shapes are rejected before router construction. Unknown paths return 404, unsupported methods return 405, oversized bodies return 413, handler/body timeouts return 504, and required-HTTPS failures return 400. The adapter provides ordered global, group, and route-scoped async middleware, uniquely named routes, route groups, deterministic metadata from `Application::routes()`, graceful shutdown APIs, local rustls serving, trusted-proxy reconstruction, safe full HTML documents, and configurable escaped 4xx/5xx error pages.

The typed form/JSON/query validation APIs, structured validation errors, `Request::redirect_back`, opt-in `CsrfProtection`, and Phase 6.4 session/access-control APIs are experimental. CSRF middleware must be explicitly attached to cookie-authenticated mutation routes; it does not itself authenticate or authorize users, and it does not replace rate limiting. `SessionManager`, `SessionContext`, `route_protected` and `route_authorized` provide session and guard primitives, but RustFlow still has no built-in user/credential provider, OAuth/OIDC, MFA, account recovery, rate limiting or durable session store. The adapter also lacks built-in subscriber/export configuration, automatic OS-signal wiring and database integration. See [FORM_VALIDATION.md](FORM_VALIDATION.md) and [ADR 0015](adr/0015-sessions-and-access-control.md). Integration tests cover HTML/JSON routes, groups, route middleware, query/path parameters, custom 404/405 pages, 413/504, request-ID headers, graceful shutdown, trusted and spoofed forwarded headers, local TLS, client aborts, malformed input, and a concurrency cap. They are not a substitute for a reverse-proxy deployment recipe or a Phase 9 threat model.

## Current SSR composition, hydration, and browser navigation

The current workspace adds page composition on top of the HTTP foundation:

- `HtmlPage` builds a full HTML document from a structural view or a registered component, sets the document language, accepts explicit component props, attaches a shared component catalog, and wraps the body with named layout components through `with_layout`.
- For concurrent HTTP routes, build `SharedComponentCatalog` once at startup and capture an `Arc` in route handlers. The catalog shares immutable factories; render runtimes, scopes, props, and view state remain request-local.
- `HydrationData` is a bounded, HTML-script-safe JSON payload. Page helpers can emit opt-in hydration markers and the payload, but the application must choose public fields explicitly.
- `DomRenderer` offers opt-in hydration for a compatible existing SSR DOM. A mismatch conservatively leaves the entire root static; granular mismatch repair is not implemented.
- `rustflow_dom::ClientNavigation` is a separate opt-in browser adapter for eligible same-origin links, History API behavior, and supported GET-form enhancement. It requires a mounted/hydrated runtime and an application-owned route callback. It does not insert fetched HTML arbitrarily.

These newer capabilities do not imply that the HTTP adapter provides a general full-stack router, server actions, sessions, authentication, integrated authorization, uploads, or production deployment readiness. CSRF is a separate opt-in middleware and must be explicitly wired to cookie-authenticated mutation routes. Read the public learning guides for task-oriented examples: [HTTP server](../website/src/content/docs/guides/server.md), [SSR and hydration](../website/src/content/docs/guides/ssr.md), [layouts](../website/src/content/docs/guides/layouts.md), [navigation](../website/src/content/docs/guides/navigation.md), and the [runnable product page example](../crates/rustflow-server/examples/product_page.rs).
