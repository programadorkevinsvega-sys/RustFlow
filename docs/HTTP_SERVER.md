# HTTP server foundation (prototype)

The `rustflow-server` crate is the HTTP adapter and composition boundary. It wraps Axum, Tokio, and Tower behind RustFlow-owned request, response, error, configuration, and route types. It is a pre-1.0 foundation, not a production deployment recipe.

## Ejecutar el ejemplo

Ejecuta `cargo run -p rustflow-server --example basic`. El servidor de ejemplo escucha en `127.0.0.1:3000` y expone `/`, `/api/health`, `/users/{id}?detailed=true` y `POST /api/echo`. Añade una cabecera mediante middleware global, registra la ruta de salud con el nombre `health`, imprime la tabla de rutas y usa `Ctrl+C` como future de apagado graceful.

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

Handlers are async closures that return `Result<Response, HttpError>`. Route handlers can capture shared state explicitly; this prototype intentionally has no hidden global service locator. Path parameters are decoded from the route template. Query strings can be decoded with `Request::query::<T>()`, and JSON bodies with `Request::json::<T>()`; invalid inputs produce generic 400 responses.

## Responses

Use `Response::text`, `Response::html`, `Response::json`, `Response::redirect`, `Response::file_download`, or `Response::status`. `Response::html` emits the supplied string as-is, so it must only receive trusted/escaped content. Prefer `html_view` or `html_view_with_components` for safe structural fragments, or `html_document(title, lang, &view)` for a complete document with doctype, UTF-8 metadata, title, and body. The component-aware document helper is `html_document_with_components`.

## Resource limits

Defaults cap buffered request bodies at 1 MiB, allow up to 256 concurrent requests, and time out body reads/handlers after 30 seconds. These are initial defaults, not a substitute for reverse-proxy, TLS, deployment, rate-limit, or application-specific protections. Use `Application::with_config(ServerConfig::new()...)` to change them.

## Middleware, route metadata, and graceful shutdown

Register middleware with `app.middleware(Middleware::new(|request, next| async move { /* before */ let response = next.run(request).await?; /* after */ Ok(response) }))`. The stack is global and ordered; middleware may short-circuit without calling `next.run`. Register a route with `route_named("users.show", HttpMethod::Get, "/users/{id}", handler)` and inspect it with `app.routes()`.

For shutdown, call `serve_with_shutdown(address, shutdown_future)` or `serve_listener_with_shutdown(listener, shutdown_future)`. The application composition root owns the trigger (for example, an OS signal future); the adapter does not silently install a process-wide signal handler.

## Request IDs and tracing

Every handled request receives a correlation ID exposed as `Request::request_id()` and echoed in the response header `x-request-id`, including body-read errors. A supplied ID is retained only if it is 1–128 ASCII letters, digits, dots, underscores, or hyphens; otherwise the server generates an ID with the `rf-` prefix. It is for log correlation only and must never be used as an authorization credential.

The adapter emits a `tracing` span named `http.request` with the request ID, HTTP method, and registered route template. It records completion/failure status through tracing events. The library does not install a global subscriber: configure `tracing-subscriber` (or an application-selected tracing layer/exporter) in the binary/composition root.

## Complete documents and configurable error pages

Use `html_document(title, lang, &view)` when a handler should return a complete HTML document rather than a fragment. It builds the document shell, UTF-8 charset metadata, title, and body structurally. The title, language attribute, and view text are escaped/validated by `rustflow-ssr`. `html_document_with_components` also resolves registered components in the body.

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

Routing supports conventional methods and Axum path templates such as `/users/{id}`. Duplicate method/path registration and equivalent parameter-only route shapes are rejected before router construction. Unknown paths return 404, unsupported methods return 405, oversized bodies return 413, and handler/body timeouts return 504. The adapter now provides ordered global async middleware through `Middleware::new` and `Next::run`, uniquely named routes with `route_named`, deterministic metadata from `Application::routes()`, `serve_with_shutdown`/`serve_listener_with_shutdown`, safe full HTML documents, and configurable escaped 4xx/5xx error pages. It still lacks route-scoped middleware, route groups, typed extraction traits, built-in subscriber/export configuration, automatic OS-signal wiring, TLS/reverse-proxy trust handling, client-abort and malformed-transport tests, concurrency/load testing, and database/session integrations. Representative real-TCP tests cover ordinary HTML/JSON routes, query/path parameters, custom 404/405 pages, 413 body limits, 504 timeouts, request-ID headers, and graceful shutdown; they do not constitute a full transport or deployment-security suite.
