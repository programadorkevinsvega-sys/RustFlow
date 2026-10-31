---
title: Catálogo de ejemplos
description: Encuentra los ejemplos ejecutables del repositorio y el caso de uso que demuestra cada uno.
---

Esta página es un índice de los ejemplos que existen en el repositorio. Los comandos se deben ejecutar desde la raíz del checkout, salvo cuando se indique otra cosa. Como RustFlow es pre-1.0, ejecuta los ejemplos de la misma revisión que estás leyendo: las firmas y las APIs pueden cambiar.

## CLI y proyecto generado

Desde un checkout del repositorio, instala el binario y crea una aplicación de servidor:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow new hello-rustflow
cd hello-rustflow
rustflow dev
~~~

Desde la raíz de la nueva aplicación, puedes generar `component ProductCard`, `page about`, `controller health`, `model User` o `migration create_users` con `rustflow generate <tipo> <nombre>`. La implementación de plantillas está en [`crates/rustflow-cli/src/lib.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-cli/src/lib.rs). Modelos y migraciones son estructurales y no integran persistencia. Más detalles en la [guía del CLI](/guides/cli/).

## Ejemplos ejecutables

| Ejemplo | Comando | Casos de uso |
|---|---|---|
| Servidor HTTP básico | `cargo run -p rustflow-server --example basic` | Página HTML, componente SSR, catálogo compartido, layout, grupos HTTP, middleware y endpoints JSON. |
| Página de producto SSR | `cargo run -p rustflow-server --example product_page` | Datos en memoria, ruta con ID de producto, 404, props públicas, catálogo compartido y layout. |
| Interfaz browser/WASM | Ver [README del browser](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) | Counter, señales, callbacks, lista de tareas, claves, montaje/desmontaje, navegación optativa y fixtures E2E de hidratación. |

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

## Verificación compartida

Desde la raíz del repositorio puedes ejecutar `cargo test --workspace --all-features`. Para comprobar lint, formato y documentación Rust, consulta [Probar y depurar aplicaciones](/guides/testing/). Para disponibilidad y límites por API, usa [Referencia de disponibilidad de API](/reference/api-status/).

## Contratos de integración de persistencia

Los ejemplos ejecutables anteriores enseñan HTTP, SSR y la interfaz browser/WASM; no son una aplicación que conecte el ORM o el ODM. La persistencia tiene contratos de integración separados:

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

Estos comandos son pruebas de integración, no demos standalone ni una aplicación CRUD generada. La aplicación CRUD que conectará las capacidades en un flujo de extremo a extremo corresponde a la Fase 6.7.5, todavía pendiente.

## Lo que los ejemplos no proporcionan

Los datos del catálogo de productos y de la lista de tareas de estos ejemplos no son persistentes. RustFlow ya incluye un ORM SQL experimental para SQLite/PostgreSQL/MySQL/MariaDB y un ODM MongoDB independiente, pero estos ejemplos no los conectan. Todavía no hay una demo canónica full-stack que integre una app CRUD generada con proveedor de usuarios, flujo de autenticación completo, autorización de negocio, CSRF y server actions. Consulta la [guía de base de datos](/guides/database/) y la [guía ODM](/guides/mongodb-odm/) para los contratos de persistencia y sus límites. No extrapoles una aplicación full-stack terminada desde ejemplos aislados de SSR, JSON o browser.


## Sesiones y rutas protegidas — Fase 6.4 integrada en main

La guía [Sesiones, autenticación y autorización](/guides/sessions-and-access/) muestra la API experimental, rotación, guards antes del handler, políticas deny-by-default e integración CSRF de login/logout. No hay proveedor de usuarios ni adaptador persistente para SessionStore. Para la nueva integración SQLx de datos de aplicación, consulta [Base de datos y migraciones](/guides/database/). Comando reproducible: `cargo test -p rustflow-server --test session_access`. Los contratos SQL y MongoDB se describen en la sección de pruebas de persistencia más abajo.
