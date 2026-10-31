# {{project_name}}

A generated RustFlow CRUD example that demonstrates form validation, the SQL ORM, explicit transactions, session-based authorization, opt-in CSRF middleware and safe server-side HTML rendering.

RustFlow and its APIs are experimental and pre-1.0. This generated example deliberately uses `InMemorySessionStore` and a development cookie policy. It refuses to start in the production environment. It is an educational integration fixture, not a production-ready authentication or deployment template.

## Start locally

Install the CLI from a RustFlow checkout, then create this project:

```sh
cargo install --path crates/rustflow-cli --locked
rustflow new {{project_name}} --template crud
cd {{project_name}}
cp .env.example .env
# Replace the example credentials and CSRF key in .env before starting.
rustflow dev
```

Open `http://127.0.0.1:3000/login`. The local demo credentials come from `.env`. After sign-in, create, list, view, update and delete products.

## What this app proves

- `Request::validated_form` and `ValidateInput` return structured 422 validation errors.
- `DatabasePool::entity` and `EntityRepository` handle entity queries and CRUD using the one shared SQLx/SeaORM pool.
- `DatabasePool::begin_transaction` combines multiple ORM writes; `/admin/transaction-rollback-probe` deliberately violates a unique SKU constraint and explicitly rolls back the first write.
- `SessionManager`, `AuthIdentity`, `route_protected` and `route_authorized` protect routes. A configured permission with no matching rule is denied.
- `CsrfProtection` is explicitly attached to cookie-authenticated POST routes. The cookie and submitted token must both be valid and requests must carry same-origin context.
- `View`, `Element` and `html_document` render pages. User-controlled text and attribute values go through the structural SSR serializer rather than raw HTML insertion.

## SQL engines

The app selects one migration directory according to `DatabaseBackend`: `migrations/sqlite`, `migrations/postgres` or `migrations/mysql`. MariaDB uses the `mysql://` URL and MySQL protocol driver. ORM operations remain engine-neutral; schema DDL and the migration files are intentionally dialect-specific. Configure `RUSTFLOW_DATABASE_URL` to select an engine and run `cargo run --bin migrate` before deploying a non-demo application.

## Important limits

- `rustflow generate model <Name>` continues to create a plain domain struct for backward compatibility. It does not infer table names, field types, keys or relations from a name. See `src/models/product.rs` for the accepted SeaORM entity plus RustFlow `ModelDiagnostics` pattern.
- The example login checks one configured demo account. RustFlow does not provide a user store, account lifecycle, OAuth/OIDC, MFA, rate limiting or durable sessions.
- `InMemorySessionStore` is volatile, process-local and unsuitable for multi-instance production deployments.
- CSRF is opt-in; applications must attach it to every cookie-authenticated unsafe route.
- Server actions, automatic ORM code generation and a full production deployment recipe remain outside this example.
