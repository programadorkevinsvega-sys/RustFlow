---
title: Probar y depurar aplicaciones
description: Ejecuta la verificación de Rust, pruebas HTTP y browser E2E, y localiza fallos sin asumir que CI despliega la app.
---

**Estado de Phase 7.3 — completada:** la infraestructura de pruebas se integró en `main` mediante PR #71 el 2026-11-01 (squash merge `ebc671b4c5dc7688813158f663a3dc560c34edb7`). El HEAD final de la PR `f3c7e5451095759428745f7760f3c531c91c89bc` pasó los checks requeridos de Rust stable/1.85, WebAssembly, Browser E2E/bundle, build del sitio, gobernanza, dependency review y Cloudflare Pages. La feature `rustflow-server/testing` continúa opt-in y experimental. El CRUD SQL completo se demuestra con la plantilla `--template crud`; el ejemplo autenticado se ejecuta por separado y no representa una configuración de producción.

La forma más fiable de aprender el contrato actual de RustFlow es ejecutar los ejemplos y las pruebas en la misma revisión del repositorio que piensas utilizar. Las APIs siguen siendo pre-1.0, y pasar las pruebas no significa que una aplicación tenga seguridad, disponibilidad o compatibilidad de producción garantizadas.

## 1. Verificar el workspace Rust

Desde la raíz del repositorio ejecuta:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Estas tareas comprueban formato, advertencias de Clippy, tests del workspace y la construcción de documentación Rust. Si falla una, corrige el primer error y repite la orden pertinente; no ocultes advertencias con allow globales solo para obtener una CI verde.

## 2. Probar el servidor HTTP

Levanta el ejemplo ejecutable:

~~~sh
cargo run -p rustflow-server --example basic
~~~

En otra terminal, prueba rutas de diferentes clases:

~~~sh
curl -i http://127.0.0.1:3000/
curl -i http://127.0.0.1:3000/articles/demo
curl -i http://127.0.0.1:3000/api/health
curl -i 'http://127.0.0.1:3000/users/42?detailed=true'
curl -i -X POST http://127.0.0.1:3000/api/echo \
  -H 'content-type: application/json' \
  -d '{"message":"hola"}'
~~~

El ejemplo demuestra documento HTML, componente SSR, catálogo y layout compartidos, JSON, parámetros de ruta/query y middleware. No escribe en una base de datos. Para la suite de integración de transporte y configuración HTTP:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~


### Test app builder y request helpers

Para probar handlers y middleware sin abrir un puerto TCP, habilita la feature optativa `testing` en `rustflow-server` como dependencia de desarrollo, usando la misma versión o ruta que la dependencia normal. La utilidad ejecuta la solicitud a través del router real de RustFlow; no llama directamente al handler ni reemplaza el pipeline HTTP.

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

`TestApp` ofrece GET, POST, PUT, PATCH, DELETE y métodos arbitrarios. El builder de solicitud puede establecer headers, cookies, bearer authorization, `X-CSRF-Token`, query parameters, JSON, formularios URL-encoded y cuerpos raw. `TestResponse` permite inspeccionar status, headers, cada valor `Set-Cookie`, redirects, body como bytes/texto y JSON tipado.

El helper es optativo (`rustflow-server` feature `testing`) y no inicia TCP. Las pruebas de TLS, `ConnectInfo`, IP de peer, proxies confiables, desconexión del cliente y graceful shutdown deben conservar el transporte real.

Comando de la suite del helper:

~~~sh
cargo test -p rustflow-server --features testing --test testing
~~~

La suite HTTP/TCP prueba rutas, datos malformados, grupos, middleware, request IDs, páginas de error, límites, timeouts, TLS local, proxies confiables, abortos de cliente y concurrencia acotada. Los casos de la Fase 6.3 incluyen formularios tipados, validación JSON/query, errores estructurados 422, content-types no admitidos, campos inesperados, CSRF ausente/inválido/válido y redirecciones seguras. Para persistencia, hay contratos de integración separados para el ORM SQL de cuatro motores, el soporte MySQL/MariaDB y el ODM MongoDB (incluidos replica set y servidor standalone); sus variables de servicio y comandos específicos se documentan en [Base de datos](/guides/database/). Consulta también la [guía de formularios y validación](/guides/forms-validation/).



## 4. Fixtures aisladas para bases de datos

La feature optativa **rustflow-server/testing** expone utilidades de persistencia:

- **TestSqlDatabase::sqlite_in_memory()** crea una SQLite en memoria nueva por test y limita el pool a una conexión para conservar el mismo esquema en todos los accesos SQLx/SeaORM.
- **TestSqlDatabase::connect_test_url(url)** exige que el nombre final de la base sea **test**, empiece por **test_** o termine en **_test**. Cada fixture genera su propio **TestNamespace** para construir nombres de tabla únicos.
- **TestMongoDatabase::connect(uri)** asigna una base MongoDB única. Llama **cleanup().await** para borrar únicamente esa base.
- Las utilidades SQL externas no crean ni eliminan bases/esquemas y nunca borran tablas ajenas. Usa URLs dedicadas de test; no apuntes a datos de desarrollo o producción.

Ejemplo SQLite:

~~~rust
use rustflow_server::testing::TestSqlDatabase;

#[tokio::test]
async fn isolated_sql_test() -> Result<(), Box<dyn std::error::Error>> {
    let database = TestSqlDatabase::sqlite_in_memory().await?;
    let table = database.namespace().table_name("items")?;
    database.pool().execute_raw(
        &format!("CREATE TABLE {table} (value TEXT NOT NULL)"),
        std::iter::empty(),
    ).await?;
    database.close().await;
    Ok(())
}
~~~

Ejemplo MongoDB:

~~~rust
use rustflow_server::testing::TestMongoDatabase;

#[tokio::test]
async fn isolated_document_test() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TestMongoDatabase::connect(
        std::env::var("RUSTFLOW_TEST_MONGODB_URI")?
    ).await?;
    fixture.database().ping().await?;
    fixture.cleanup().await?;
    Ok(())
}
~~~

Para motores SQL externos configura **RUSTFLOW_TEST_DATABASE_URL** (PostgreSQL), **RUSTFLOW_TEST_MYSQL_URL** y **RUSTFLOW_TEST_MARIADB_URL**, siempre con una base dedicada de test. Para MongoDB utiliza **RUSTFLOW_TEST_MONGODB_URI**; el contrato transaccional requiere replica set. Los tests aislados y sus límites están descritos en la guía técnica de testing del repositorio.

## 5. Ejemplo autenticado completo

El ejemplo ejecutable **cargo run -p rustflow-server --example authenticated** muestra sesiones server-side, verificación de contraseña con **PasswordHash**, identidad y roles de origen servidor, rutas protegidas, política deny-by-default y CSRF para login/logout. Requiere **RUSTFLOW_DEMO_USER**, **RUSTFLOW_DEMO_PASSWORD** y **RUSTFLOW_CSRF_KEY** (mínimo 32 bytes); escucha en **127.0.0.1:3001** por defecto.

Rutas: **GET /csrf** emite token y cookie CSRF; **POST /login** valida credenciales JSON y rota la sesión; **GET /me** exige identidad; **GET /admin** requiere el permiso **admin.read**; **POST /logout** valida CSRF e invalida la sesión. El ejemplo se niega a arrancar con **RUSTFLOW_ENV=production**.

Este es un ejemplo educativo: usa **InMemorySessionStore** y una cookie de desarrollo, así que no es una implementación de autenticación lista para producción ni una receta de despliegue.

## 6. Compilar y probar el navegador WebAssembly

Desde la raíz:

~~~sh
cd examples/browser
npm install
npm run build
npm run check:bundle
npm run dev
~~~

Para ejecutar E2E instala Chromium y corre Playwright:

~~~sh
npx playwright install --with-deps chromium
npm run test:e2e
~~~


La suite utiliza los fixtures incorporados de Playwright: cada test recibe un BrowserContext y una página nuevos, y Playwright cierra ese contexto al terminar, también cuando un test falla. RustFlow añade `mountApplication(page, path)` para navegar y esperar a que el root `#app` esté visible, y el fixture `browserErrors` recoge errores de consola y excepciones de página. La limpieza de listeners se ejecuta en `finally`; evita compartir `page`, cookies o storage entre tests.

El helper de montaje espera una condición observable del DOM; no utiliza sleeps. Para esperar reactividad o navegación, prefiere las assertions auto-waiting de Playwright (`expect(locator).toHaveText(...)`) sobre pausas temporizadas.

La demo cubre el contador, eventos, lista de tareas, identidad de nodos keyed, montaje/desmontaje y navegación cliente optativa. Las fixtures E2E prueban también hidratación sobre HTML preparado para ese contrato. Esto no convierte la demo completa en una aplicación servida por SSR.

Si falla la compilación de WASM, verifica el target y las herramientas antes de investigar el código:

~~~sh
rustup target add wasm32-unknown-unknown
cargo check -p rustflow-dom --target wasm32-unknown-unknown
~~~

## 7. Diagnóstico rápido

| Síntoma | Comprobación inicial |
|---|---|
| Un snippet no compila | Compara la firma con la revisión de RustFlow; el website explica la API, pero no todos los fragmentos son programas independientes. |
| No se actualiza una vista | Comprueba que la lectura se rastrea como señal y que las expresiones arbitrarias están dentro de una región dinámica compatible. |
| Falla un componente | Revisa props requeridas/tipos, registro del componente y compatibilidad del target de render. |
| Una respuesta HTML falla | Separa fragmento SSR de documento completo y revisa tags/atributos rechazados por la allowlist. |
| No intercepta un enlace | Confirma origen, target, modificadores, fragmento, download y exclusiones descritas en [Navegación](/guides/navigation/). |
| Un formulario sigue enviándose de forma normal | Es el comportamiento esperado si no lleva opt-in o usa un método/codificación/control no compatible. |
| Falla E2E antes de abrir el navegador | Instala la versión de Chromium que requiere Playwright y revisa el output del runner/trace. |
| Un ejemplo no es persistente | El ejemplo mantiene sus datos en memoria; no se debe inferir almacenamiento integrado. |

## 5. ¿Qué significa una CI verde?

Una CI verde significa que los jobs definidos para esa revisión pasaron. No comprueba automáticamente los dominios, variables, certificados, secretos, políticas de acceso, persistencia o escalado de tu despliegue. Por separado, la publicación del website depende de la acción de documentación y del proveedor de Pages.

Para ejecutar localmente los contratos de base de datos, configura los servicios desechables correspondientes y sus variables `RUSTFLOW_TEST_DATABASE_URL`, `RUSTFLOW_TEST_MYSQL_URL`, `RUSTFLOW_TEST_MARIADB_URL`, `RUSTFLOW_TEST_MONGODB_URI` y `RUSTFLOW_TEST_MONGODB_STANDALONE_URI`; los tests de servicios externos se omiten fuera de CI cuando falta una URL. Comandos específicos: `cargo test -p rustflow-server --test orm_contract -- --test-threads=1`, `cargo test -p rustflow-server --test database_engines -- --test-threads=1` y `cargo test -p rustflow-server --test mongodb_adapter -- --test-threads=1`.

Antes de presentar una nueva guía como ejemplo canónico, verifica que el comando existe, que la API está implementada, que el ejemplo relevante compila o tiene una prueba que lo cubre y que se explican los límites conocidos. Sigue las [normas de documentación](/contributing/documentation/).

## Referencias

- [Empezar con RustFlow](/getting-started/setup/)
- [Catálogo de ejemplos](/reference/examples/)
- [README del ejemplo browser](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md)
- [Workflow de CI](https://github.com/programadorkevinsvega-sys/RustFlow/actions)
