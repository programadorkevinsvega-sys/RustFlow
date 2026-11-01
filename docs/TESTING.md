# RustFlow testing contract

This document describes the repository's application-testing support. RustFlow remains
pre-1.0: the test helpers are opt-in developer tooling, not a stability promise for
the framework's application APIs.

## In-process HTTP applications

The rustflow-server crate exposes the `testing` Cargo feature. Enable it only
where tests need it, preferably as a dev-dependency, using the same crate version or
path as the application's runtime dependency.

The feature exposes `rustflow_server::testing::{TestApp, TestAppBuilder}`.
It lives inside the server adapter because that module assembles the same private
router used by Application::serve; it does not implement a parallel request pipeline
and adds no new runtime dependency. With the feature disabled, the testing module is
not compiled or exported.

~~~rust
use http::StatusCode;
use rustflow_server::{Request, Response, testing::TestApp};

#[tokio::test]
async fn health_route() -> Result<(), Box<dyn std::error::Error>> {
    let app = TestApp::builder()
        .configure(|application| {
            application.get("/health", |_request: Request| async {
                Ok(Response::text("ok"))
            })?;
            Ok(())
        })
        .build()?;

    let response = app.get("/health").send().await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text()?, "ok");
    Ok(())
}
~~~

TestAppBuilder accepts an existing Application or ordered configuration closures,
a ServerConfig for a new application, and a bounded response-capture limit.
Handlers, route groups, global/route middleware, state captured by closures, error
pages and request/body limits are supplied by the real Application. Configuration
errors fail build rather than returning a partially configured app.

TestApp provides GET, POST, PUT, PATCH, DELETE and arbitrary-method request builders.
Requests support headers, Accept, Content-Type, cookies, bearer authorization, the
X-CSRF-Token header, serialized query parameters, JSON, URL-encoded forms and raw
bodies. TestResponse exposes status, headers, separate Set-Cookie values, redirect
location, bounded body bytes, UTF-8 text and typed JSON deserialization.

The helper dispatches directly through the actual Axum router using Tower's in-process
service interface. It does not open a listener and does not change route, middleware,
validation, request parsing or error-response semantics.

### What in-process tests do not verify

An in-process request has no TCP peer address. It cannot establish TLS, exercise
trusted-proxy/client-address behavior, verify connection lifecycle, test client
disconnects, or validate graceful shutdown/cancellation. Keep the existing TCP tests
for those behaviors. Do not replace transport integration coverage with TestApp.

## Database and browser test isolation

Database and browser test utilities are tracked separately in Phase 7.3. The existence
of TestApp does not imply that SQL or MongoDB fixtures are isolated, nor that browser
state is reset. Continue using the explicit database service URLs and Playwright
browser E2E workflow until those contracts are implemented and documented.

## Reproducible baseline commands

From the repository root:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo test -p rustflow-server --features testing --test testing
~~~

The explicit test-support command requires the optional testing feature. The
workspace all-features command also compiles and runs this test target.
