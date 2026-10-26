# HTTP server foundation (prototype)

The `rustflow-server` crate is the HTTP adapter and composition boundary. It wraps Axum, Tokio, and Tower behind RustFlow-owned request, response, error, configuration, and route types. It is a pre-1.0 foundation, not a production deployment recipe.

## Ejecutar el ejemplo

Ejecuta `cargo run -p rustflow-server --example basic`. El servidor de ejemplo escucha en `127.0.0.1:3000` y expone `/`, `/api/health`, `/users/{id}?detailed=true` y `POST /api/echo`.

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

Use `Response::text`, `Response::html`, `Response::json`, `Response::redirect`, `Response::file_download`, or `Response::status`. `Response::html` emits the supplied string as-is, so it must only receive trusted/escaped content. Prefer `html_view` or `html_view_with_components` for structural views; they use `rustflow-ssr` and map render errors to a generic 500 response.

## Resource limits

Defaults cap buffered request bodies at 1 MiB, allow up to 256 concurrent requests, and time out body reads/handlers after 30 seconds. These are initial defaults, not a substitute for reverse-proxy, TLS, deployment, rate-limit, or application-specific protections. Use `Application::with_config(ServerConfig::new()...)` to change them.

## Middleware, route metadata, and graceful shutdown

Register middleware with `app.middleware(Middleware::new(|request, next| async move { /* before */ let response = next.run(request).await?; /* after */ Ok(response) }))`. The stack is global and ordered; middleware may short-circuit without calling `next.run`. Register a route with `route_named("users.show", HttpMethod::Get, "/users/{id}", handler)` and inspect it with `app.routes()`.

For shutdown, call `serve_with_shutdown(address, shutdown_future)` or `serve_listener_with_shutdown(listener, shutdown_future)`. The application composition root owns the trigger (for example, an OS signal future); the adapter does not silently install a process-wide signal handler.

## Boundaries and limitations

Routing supports conventional methods and Axum path templates such as `/users/{id}`. Duplicate method/path registration and equivalent parameter-only route shapes are rejected before router construction. Unknown paths return 404, unsupported methods return 405, oversized bodies return 413, and handler/body timeouts return 504. The adapter now provides ordered global async middleware through `Middleware::new` and `Next::run`, uniquely named routes with `route_named`, deterministic metadata from `Application::routes()`, and `serve_with_shutdown`/`serve_listener_with_shutdown` APIs that accept a caller-owned shutdown future. It still lacks route-scoped middleware, route groups, typed extraction traits, built-in tracing/logging, automatic OS-signal wiring, broad network-level integration tests, and database/session integrations.
