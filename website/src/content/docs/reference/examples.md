---
title: Catálogo de ejemplos
description: Encuentra los ejemplos ejecutables del repositorio y el caso de uso que demuestra cada uno.
---

Esta página es un índice de los ejemplos que existen en el repositorio. Los comandos se deben ejecutar desde la raíz del checkout, salvo cuando se indique otra cosa. Como RustFlow es pre-1.0, ejecuta los ejemplos de la misma revisión que estás leyendo: las firmas y las APIs pueden cambiar.

## CLI y proyecto generado

Desde un checkout del repositorio, instala el binario y crea una aplicación de servidor (plantilla predeterminada) o una aplicación CRUD SQL:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow new hello-rustflow
# Alternativa: rustflow new products --template crud
cd hello-rustflow
rustflow dev
~~~

Desde la raíz de la aplicación, puedes generar `component ProductCard`, `page about`, `controller health`, `model User` o `migration create_users` con `rustflow generate <tipo> <nombre>`. El generador `model` conserva su struct plano para compatibilidad. La plantilla `crud` incluye una entidad SeaORM explícita y migraciones por dialecto. Más detalles en la [guía del CLI](/guides/cli/).

## Ejemplos ejecutables

| Ejemplo | Comando | Casos de uso |
|---|---|---|
| Servidor HTTP básico | `cargo run -p rustflow-server --example basic` | Página HTML, componente SSR, catálogo compartido, layout, grupos HTTP, middleware y endpoints JSON. |
| Página de producto SSR | `cargo run -p rustflow-server --example product_page` | Datos en memoria, ruta con ID de producto, 404, props públicas, catálogo compartido y layout. |
| CRUD SQL completo (plantilla CLI) | `rustflow new products --template crud` | Listar/detallar/crear/editar/eliminar, relación Category/Product, filtros/paginación, ORM SQL, validación, transacciones, guards, CSRF y SSR. La prueba CLI ejecuta el flujo HTTP real. |
| Aplicación autenticada | `cargo run -p rustflow-server --example authenticated` | Login con hash PBKDF2, sesiones, identidad, rol, guard, autorización por permiso, CSRF y logout. Demo de desarrollo, no producción. |
| Soporte HTTP de testing | `cargo test -p rustflow-server --features testing --test testing` | Ejecuta requests contra el router real en-process; comprueba métodos, headers, cookies, formularios, middleware y redirects sin abrir un puerto TCP. Feature opt-in y experimental. |
| Interfaz browser/WASM | Ver [README del browser](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) | Counter, señales, callbacks, lista de tareas, claves, montaje/desmontaje, navegación optativa y fixtures E2E de hidratación. |

## Infraestructura de fixtures

La feature optativa `rustflow-server/testing` también expone fixtures para SQLite in-memory, tablas namespaced en bases SQL externas dedicadas y bases MongoDB únicas con limpieza explícita. Los contratos, variables de entorno y límites de aislamiento están en [Probar y depurar aplicaciones](/guides/testing/) y [docs/TESTING.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/TESTING.md). Los helpers Playwright de browser están en `examples/browser/tests/support/fixtures.js` y se usan desde los tests E2E del ejemplo browser.

## 1. Servidor HTTP básico

El archivo canónico es [`crates/rustflow-server/examples/basic.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs). Arranca en localhost:3000 y permite probar:

- `/`: documento HTML generado desde una vista estructural.
- `/articles/demo`: componente SSR y layout de catálogo compartido.
- `/api/health`: respuesta JSON.
- `/users/42?detailed=true`: parámetro de ruta y query deserializada.
- `POST /api/echo`: cuerpo JSON y respuesta JSON.

Tutorial: [Construir una API JSON](/tutorials/json-api/). Guías: [Servidor HTTP](/guides/server/) y [Errores, límites y seguridad](/guides/errors-and-security/).

## 2. Página de producto SSR

El ejemplo [`product_page.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs) es un servidor separado, también en el puerto local 3000. Detén el servidor básico antes de iniciarlo. Las rutas `/products/keyboard` y `/products/mouse` enseñan un catálogo fijo y cómo se convierte un registro encontrado en props públicas.

Tutorial: [Página de producto SSR](/tutorials/product-page/). Guías: [SSR](/guides/ssr/) y [Layouts](/guides/layouts/).

## 3. Interfaz reactiva browser/WASM

El ejemplo de [`examples/browser`](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/examples/browser) se ejecuta con Vite y Rust compilado a WebAssembly. Consulta su README para requisitos de Rust, target WASM, Node/npm, build, presupuesto de bundle y Playwright E2E.

Tutorial: [Interfaz reactiva de tareas](/tutorials/reactive-todo/). Guías: [Estado y reactividad](/concepts/reactivity/), [Componentes](/guides/components/) y [Navegación](/guides/navigation/).

## Ejemplos SSR, layouts y rutas JSON

La composición SSR con catálogo compartido y layout está en [`crates/rustflow-server/examples/basic.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs) y [`product_page.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs). Para JSON sigue el [tutorial de API JSON](/tutorials/json-api/). Para UI reactiva de navegador usa [`examples/browser/src/app.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/src/app.rs), no el scaffold de servidor.

## 4. Formularios y validación del servidor

La guía de [Formularios y validación](/guides/forms-validation/) contiene ejemplos de extracción tipada de formularios URL-encoded, validación con `ValidateInput`, errores HTTP 422 separados por campo y formulario, protección CSRF optativa para mutaciones que usan cookies y redirección segura de retorno. También explica los estados 400, 413 y 415 y los límites de la API.

No hay todavía un servidor de demostración independiente dedicado a formularios; no confundas los fragmentos de la guía con una app ejecutable completa. El contrato de red se prueba en la suite de integración TCP canónica:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

Esa suite incluye `real_tcp_typed_forms_validate_and_return_structured_errors`, `real_tcp_rejects_oversized_payloads_before_handler_execution`, `real_tcp_csrf_middleware_requires_a_valid_cookie_and_matching_token` y `real_tcp_redirect_back_accepts_same_origin_and_falls_back_for_external_referers`. El archivo fuente está en [`http_integration.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/tests/http_integration.rs).

## Aplicación CRUD generada (SQL ORM)

Genera y ejecuta el proyecto:

~~~sh
rustflow new products --template crud
cd products
cp .env.example .env
# Cambia el usuario, la contraseña demo y RUSTFLOW_CSRF_KEY.
rustflow dev
~~~

Abre `/login`, inicia sesión con los valores configurados y navega por el CRUD. La aplicación usa `DatabasePool::entity`/`EntityRepository` para create/read/update/delete; `ValidateInput` para formularios; `begin_transaction` más rollback explícito para la prueba de fallo; `SessionManager`, guards y `RolePolicy` para acceso; CSRF explícito en rutas POST; y `View`/`html_document` para SSR con escape.

La app crea migraciones separadas para SQLite, PostgreSQL y MySQL/MariaDB. Cambiar el motor requiere una URL adecuada y el esquema SQL de su dialecto. La aplicación no utiliza MongoDB: el ODM documental permanece separado.

La prueba canónica genera el proyecto sin reemplazar manualmente sus fuentes, compila el binario y recorre el flujo HTTP/ORM contra SQLite:

~~~sh
cargo test -p rustflow-cli generated_crud_template_builds_and_runs_the_real_orm_application -- --nocapture
~~~

El ejemplo usa `InMemorySessionStore`, credenciales demo y cookie de desarrollo. Rechaza `RUSTFLOW_ENV=production`; no representa autenticación durable ni una aplicación lista para producción.


## 5. Aplicación autenticada

El archivo ejecutable es [authenticated.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/authenticated.rs). Ejecuta el servidor desde la raíz del repositorio:

~~~sh
export RUSTFLOW_ENV=development
export RUSTFLOW_DEMO_USER=admin
export RUSTFLOW_DEMO_PASSWORD='<contraseña local de demostración>'
export RUSTFLOW_CSRF_KEY="$(openssl rand -hex 32)"
cargo run -p rustflow-server --example authenticated
~~~

Escucha en http://127.0.0.1:3001 por defecto (puerto configurable mediante RUSTFLOW_AUTH_PORT). El flujo expone:

- GET /csrf: emite un token CSRF y la cookie HttpOnly correspondiente.
- POST /login: verifica credenciales JSON con PasswordHash, luego rota la sesión y asigna identidad/rol desde el servidor.
- GET /me: requiere identidad autenticada.
- GET /admin: requiere el permiso admin.read, concedido al rol admin por RolePolicy.
- POST /logout: exige CSRF e invalida la sesión.

La prueba ejecutable del ejemplo verifica credenciales incorrectas/correctas, acceso anónimo, identidad, autorización, CSRF y logout:

~~~sh
cargo test -p rustflow-server --features testing --example authenticated
~~~

**Límite deliberado:** usa InMemorySessionStore y SessionCookieConfig::development, no un proveedor de usuarios ni almacenamiento durable. El binario se niega a arrancar con RUSTFLOW_ENV=production; no representa una configuración lista para producción.

## Verificación compartida

Desde la raíz del repositorio puedes ejecutar `cargo test --workspace --all-features`. Para comprobar lint, formato y documentación Rust, consulta [Probar y depurar aplicaciones](/guides/testing/). Para disponibilidad y límites por API, usa [Referencia de disponibilidad de API](/reference/api-status/).

## Contratos de integración de persistencia

Los ejemplos HTTP básico, SSR de producto y browser/WASM son independientes y no conectan el ORM/ODM. La nueva plantilla CRUD es el ejemplo integrado SQL; la persistencia también conserva contratos de integración independientes:

### SQLx y ORM SeaORM

Desde la raíz del repositorio:

~~~sh
cargo test -p rustflow-server --test orm_contract -- --test-threads=1
cargo test -p rustflow-server --test database_engines -- --test-threads=1
~~~

La suite `orm_contract` cubre el contrato del ORM y sus loaders explícitos, incluidos los grafos y las proyecciones relacionales de la Fase 6.7.4. Para los casos que usan servicios externos, configura `RUSTFLOW_TEST_DATABASE_URL` (PostgreSQL), `RUSTFLOW_TEST_MYSQL_URL` y `RUSTFLOW_TEST_MARIADB_URL`; la suite también incluye casos SQLite locales. `database_engines` comprueba los servidores MySQL y MariaDB contra los targets fijados por CI.

### ODM MongoDB

~~~sh
cargo test -p rustflow-server --test mongodb_adapter -- --test-threads=1
~~~

La suite requiere un replica set mediante `RUSTFLOW_TEST_MONGODB_URI` y un servidor standalone mediante `RUSTFLOW_TEST_MONGODB_STANDALONE_URI` para probar contratos distintos, incluidas las transacciones en replica set y el rechazo de transacciones multi-documento en standalone. Consulta [Base de datos](/guides/database/) y [ODM MongoDB](/guides/mongodb-odm/) para configuración y límites.

Estos comandos ejercitan los contratos del ORM y ODM de forma aislada; el ejemplo CRUD anterior conecta la superficie ORM con HTTP. Phase 6.7.5 quedó aceptada mediante PR #64 (`edd665ad452c769c78ba8190c233e349a44b054a`); Phase 6.7.6 completó el gate global mediante PR #66 (HEAD `b049d96491e2b23bfe794ac3c0ed9abb68f7e493`, merge `6046ae7ca60843d145476177e698162b57939c68`). El scaffold prueba Category/Product, carga explícita, filtros tipados y paginación con los cuatro motores SQL; todos los checks requeridos pasaron en Rust 1.85/stable, WASM, Browser E2E/bundle, website, governance y dependency review. Las APIs ORM/ODM siguen siendo experimentales y pre-1.0.

## Lo que los ejemplos no proporcionan

Los datos del catálogo de la página de producto SSR y de la lista de tareas browser/WASM no son persistentes. La plantilla CRUD usa el ORM SQL experimental para SQLite/PostgreSQL/MySQL/MariaDB; el ODM MongoDB permanece independiente y no se mezcla en esa aplicación. La plantilla CRUD es una integración educativa de ORM, formularios, sesión volátil, autorización, CSRF y SSR, pero no ofrece proveedor de usuarios, almacenamiento durable, server actions ni una receta de producción. Consulta las guías de base de datos y sesiones para sus contratos y límites; no extrapoles preparación de producción desde la demo.


## Sesiones y rutas protegidas — Fase 6.4 integrada en main

La guía [Sesiones, autenticación y autorización](/guides/sessions-and-access/) muestra la API experimental, rotación, guards antes del handler, políticas deny-by-default e integración CSRF de login/logout. No hay proveedor de usuarios ni adaptador persistente para SessionStore. Para la nueva integración SQLx de datos de aplicación, consulta [Base de datos y migraciones](/guides/database/). Comando reproducible: `cargo test -p rustflow-server --test session_access`. Los contratos SQL y MongoDB se describen en la sección de pruebas de persistencia más abajo.
