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

## SQL test databases

TestSqlDatabase and TestNamespace are exposed by the same opt-in testing feature.

- TestSqlDatabase::sqlite_in_memory creates a fresh SQLite database for a test and
  constrains its pool to one connection, so the in-memory database is not split across
  independent SQLite connections.
- TestSqlDatabase::connect_test_url accepts only an explicitly marked test database:
  its final database name must be test, start with test_, or end with _test.
- Each fixture receives a collision-resistant TestNamespace. Use table_name("items")
  to create a DDL identifier that is checked for safe ASCII identifier characters and
  the 64-byte MySQL table-name limit.
- The external fixture does not create/drop databases or schemas. It never drops
  migration metadata or another test's tables. Tests must create and drop only the
  table names returned by their own namespace.
- Call close().await when the test is finished. SQLx and SeaORM use the same pool.

~~~rust
use rustflow_server::testing::TestSqlDatabase;

#[tokio::test]
async fn isolated_sql_test() -> Result<(), Box<dyn std::error::Error>> {
    let database = TestSqlDatabase::sqlite_in_memory().await?;
    let table = database.namespace().table_name("items")?;

    database
        .pool()
        .execute_raw(
            &format!("CREATE TABLE {table} (value TEXT NOT NULL)"),
            std::iter::empty(),
        )
        .await?;

    database.close().await;
    Ok(())
}
~~~

For external engines, provide a dedicated test URL such as
RUSTFLOW_TEST_DATABASE_URL=postgres://.../rustflow_test,
RUSTFLOW_TEST_MYSQL_URL=mysql://.../rustflow_test, or
RUSTFLOW_TEST_MARIADB_URL=mysql://.../rustflow_test. The namespace protects test-owned
tables from other test invocations; it does not turn a production database into a safe
test target. Never point these helpers at application data.

## MongoDB test databases

TestMongoDatabase.connect(uri) allocates a unique database name for each fixture and
connects through the same MongoDatabase adapter used by applications. The caller can
use native collection APIs or the typed ODM through database(). Call cleanup().await
to drop only that fixture database. Cleanup is explicit; tests should arrange it on
both success and failure paths when possible.

~~~rust
use rustflow_server::testing::TestMongoDatabase;

#[tokio::test]
async fn isolated_document_test() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TestMongoDatabase::connect(std::env::var("RUSTFLOW_TEST_MONGODB_URI")?)
        .await?;
    fixture.database().ping().await?;
    fixture.cleanup().await?;
    Ok(())
}
~~~

The fixture does not delete collections or databases selected outside its generated
namespace. The MongoDB test URI must point to a dedicated test service; CI uses a
replica-set URL for transaction tests and a separate standalone URL for the
standalone-server contract.

## Browser test isolation

The browser example uses Playwright's built-in page/context fixtures. Each test gets a
fresh BrowserContext and page, and Playwright closes that context after the test,
including when the test fails. RustFlow's examples/browser/tests/support/fixtures.js
adds mountApplication(page, path), which navigates and waits for the app root (#app),
and browserErrors, which captures console errors and uncaught page exceptions and
detaches its listeners in a finally block.

The fixture does not share cookies, local storage or page state between tests. Prefer
Playwright's auto-waiting assertions over fixed sleeps. The browser fixture does not
replace the browser E2E suite or the actual WASM build.

## Reproducible verification commands

From the repository root:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p rustflow-server --features testing --test testing
cargo test -p rustflow-server --features testing --test testing_database -- --test-threads=1
cargo test -p rustflow-server --features testing --example authenticated
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

The testing module is compiled only with the opt-in feature. The workspace all-features
command includes it; external SQL/MongoDB integration checks require the dedicated
service URLs described above. TCP integration tests remain required for transport
behavior.
