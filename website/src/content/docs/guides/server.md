---
title: Servidor HTTP
description: Construye rutas y handlers y comprende las capacidades actuales del adapter HTTP.
---

El crate <code>rustflow-server</code> integra Axum, Tokio y Tower detrás de tipos propios de RustFlow. Su objetivo es ofrecer un punto de composición de peticiones HTTP con rutas, middleware y respuestas controladas por la aplicación. Es un **prototipo pre-1.0**, no una plataforma de despliegue lista para producción.

Si empiezas una aplicación nueva, el [CLI experimental](/guides/cli/) crea un scaffold de servidor con una ruta HTML. No configura automáticamente el catálogo compartido, los layouts ni los endpoints JSON de este ejemplo; ejecútalo para aprender esas APIs.

## 1. Ejecutar el ejemplo

Desde la raíz del repositorio:

~~~sh
cargo run -p rustflow-server --example basic
~~~

El servidor escucha en <code>127.0.0.1:3000</code>. Prueba:

| URL o ruta | Qué demuestra |
|---|---|
| <code>/</code> | Documento HTML generado desde una vista estructural. |
| <code>/articles/demo</code> | Props de ruta, componente SSR, catálogo compartido y layout. |
| <code>/api/health</code> | Ruta nombrada y respuesta JSON. |
| <code>/users/42?detailed=true</code> | Parámetro de ruta y query tipada. |
| <code>POST /api/echo</code> | Decodificación de JSON y respuesta JSON. |

## 2. Crear una ruta

~~~rust
use rustflow::view::View;
use rustflow_server::{Application, HttpError, Request, html_document};

let mut app = Application::new();

app.get("/about", |_request: Request| async {
    let heading = View::element("h1", [View::text("Acerca de")])
        .map_err(|_| HttpError::internal_server_error())?;
    let body = View::element("main", [heading])
        .map_err(|_| HttpError::internal_server_error())?;
    html_document("Acerca de", "es", &body)
})?;
~~~

Una ruta asocia método y patrón de path con un handler asíncrono. El handler recibe un <code>Request</code> y devuelve un resultado con una <code>Response</code> o un error HTTP. Las vistas estructurales mantienen el contenido separado del documento y permiten que SSR aplique la política de escape.

## 3. Recibir path, query y JSON

- Usa <code>request.path_param("id")</code> para leer un parámetro nombrado en el patrón.
- Usa <code>request.query::&lt;T&gt;()</code> para decodificar query string en una estructura tipada con Serde.
- Usa <code>request.json::&lt;T&gt;()</code> para leer y decodificar un cuerpo JSON.
- Los inputs inválidos se convierten en respuestas de error genéricas del servidor, con límites de body configurables.

El ejemplo del repositorio contiene una ruta de usuario con query tipada y un endpoint que recibe un cuerpo JSON. Úsalo para verificar los nombres y tipos exactos de la API de la revisión que estás usando.

## 4. Respuestas disponibles

El adapter ofrece helpers para texto, HTML, JSON, redirecciones, descargas de bytes y códigos de estado, además de documentos HTML completos generados con <code>html_document</code>. <code>Response::html(String)</code> emite la cadena tal como se recibió: no la uses con entrada de usuario ni con HTML no escapado. Para contenido estructural prefiere vistas y SSR.

Para páginas 4xx/5xx puedes configurar páginas de error cuyos títulos y mensajes se insertan como texto escapado. El estado HTTP original y el ID de petición se conservan.

## 5. Grupos y middleware

Agrupa endpoints relacionados para compartir prefijos y middleware:

~~~rust
app.group("/api", |api| {
    api.name_prefix("api")?;
    api.get("/health", |_request: Request| async {
        Ok(rustflow_server::Response::text("ok"))
    })?;
    Ok(())
})?;
~~~

El adapter permite middleware global, por grupo y por ruta. El orden documentado es global, grupo y ruta; un middleware puede continuar la cadena o responder antes de invocar al siguiente handler. Las rutas nombradas se pueden inspeccionar mediante <code>app.routes()</code>.

## 6. Límites y manejo de recursos

La configuración del servidor establece límites de body (1 MiB por defecto), timeout de lectura/handler (30 segundos), concurrencia máxima (256) y drenaje máximo de graceful shutdown (30 segundos). Los valores de timeout de HTTP no reemplazan los límites independientes de SQLx, MongoDB o las comprobaciones de readiness. El servidor genera o valida un ID de petición, lo expone al handler y lo devuelve en <code>x-request-id</code>.

El adapter emite spans/events de <code>tracing</code> con request ID, método, plantilla de ruta, status y duración medida. No registra headers, cookies, autorización, query values ni cuerpos de petición. El binario configura exactamente un subscriber/exporter propio usando <code>TracingConfig</code> y <code>RUSTFLOW_LOG_LEVEL</code>; el framework no inicializa un subscriber global.

El API de apagado graceful recibe una future desde el punto de composición de la aplicación; la integración con señales del sistema operativo corresponde al caller. Cuando el future termina, el servidor deja de aceptar trabajo nuevo y drena conexiones activas hasta el límite configurable. Si vence, devuelve <code>ServerError::ShutdownTimedOut</code> y cancela el trabajo restante. TLS local está implementado con rustls. La política de proxies de confianza requiere que el proxy sanee las cabeceras reenviadas y que el enlace hacia el origen esté protegido.

Estos controles son una base de ingeniería, no reemplazan rate limiting, revisión de amenazas, autenticación, autorización, hardening del reverse proxy ni una receta de producción. En mutaciones autenticadas mediante cookies, conecta además el middleware CSRF optativo a las rutas correspondientes; no sustituye autenticación o autorización.

## 7. Health, readiness y error reporting

Las rutas operativas del framework son opt-in para no apropiarse de URLs que la aplicación ya pueda utilizar:

~~~rust
use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use rustflow_server::{
    Application, ReadinessCheck, ReadinessCheckError,
};

let database_available = Arc::new(AtomicBool::new(true));
let probe_state = Arc::clone(&database_available);
let mut app = Application::new();

app.enable_health_endpoints()?;
app.readiness_check(ReadinessCheck::new(
    "database",
    Duration::from_secs(2),
    move || {
        let probe_state = Arc::clone(&probe_state);
        async move {
            if probe_state.load(Ordering::Relaxed) {
                Ok(())
            } else {
                Err(ReadinessCheckError::unavailable())
            }
        }
    },
)?)?;
~~~

<code>/health/live</code> comprueba que el servidor HTTP responde y no consulta servicios externos. <code>/health/ready</code> ejecuta únicamente las comprobaciones registradas explícitamente. Las comprobaciones corren concurrentemente y cada una tiene un timeout; se permiten hasta 32. Un fallo, panic o timeout produce <code>503 Service Unavailable</code> con <code>{"status":"not_ready"}</code>. La respuesta no incluye nombres de checks ni errores de proveedores y lleva <code>Cache-Control: no-store</code>. Sin checks registrados, readiness se considera disponible.

Los callbacks deben ser de solo lectura, idempotentes y comprobar únicamente dependencias necesarias para atender tráfico en esa instancia. No ejecutes migraciones ni modifiques datos desde liveness/readiness. Los ejemplos anteriores usan un estado atómico solo para mostrar la firma del callback; en una aplicación real, sustituye el bloque por una comprobación bounded de la conexión/servicio obligatorio.

Para reportar errores internos sin exponerlos al cliente, registra un reporter de aplicación:

~~~rust
use rustflow_server::{Application, ErrorReport, ErrorReporterError};

app.with_error_reporter(|report: ErrorReport| async move {
    tracing::error!(
        request_id = %report.request_id(),
        method = %report.method(),
        route = %report.route(),
        status = %report.status(),
        "internal application failure"
    );
    Ok::<(), ErrorReporterError>(())
});
app.with_error_report_timeout(Duration::from_millis(100));
~~~

El reporter recibe solo request ID, método, plantilla de ruta, status y una categoría segura (`handler`, `middleware` o `request_timeout`). No recibe mensajes, stack traces, headers, cookies, tokens, query values ni cuerpos. Se llama para respuestas 5xx, no para fallos de entrada 4xx. El límite predeterminado es 250 ms; fallos, panics o timeouts del reporter no cambian la respuesta original. El reporter se ejecuta en una tarea aislada y debe evitar bloquear hilos del runtime o crear trabajo detached ilimitado.

Las integraciones de caché, queue, email y storage siguen diferidas: no hay contratos suficientes de invalidación/consistencia, entrega/durabilidad, aceptación frente a entrega, permisos, aislamiento y eliminación. No se exponen traits vacíos.

## 8. SSR, páginas y layouts

El servidor también integra:

- <code>rustflow_ssr</code> para serializar árboles de vistas como fragmentos HTML escapados.
- <code>html_document(title, lang, view)</code> para respuestas de documento completo.
- <code>HtmlPage</code> para página con título/idioma, componente de cuerpo, catálogo y layouts.
- <code>SharedComponentCatalog</code> para compartir definiciones inmutables entre peticiones concurrentes, con runtime y estado local por render.
- <code>HydrationData</code> y marcado opt-in para páginas que se preparan para hidratación.
- APIs de navegador separadas para hidratación y navegación cliente.

La hidratación y la navegación existen como capacidades explícitas y optativas; no se activan automáticamente en cada handler. Consulta [SSR y documentos HTML](/guides/ssr/) y [Composición de layouts](/guides/layouts/) para ver ejemplos de uso y los límites de cada mecanismo.

## 9. Verificar los endpoints

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

La suite cubre rutas por TCP local, query/path/JSON/formularios tipados, errores de validación 422, campos inesperados, CSRF, redirección de retorno, grupos, middleware, IDs de petición, error pages, liveness/readiness, error reporting, límites, timeouts, shutdown con deadline, TLS local, proxies de confianza, abortos de cliente, entradas malformadas y concurrencia acotada. Ejecuta también la suite completa del workspace antes de depender de un cambio.

Para practicar peticiones reales, ejecuta el [tutorial de API JSON](/tutorials/json-api/). Antes de exponer un servicio, revisa [errores, límites de recursos y seguridad HTTP](/guides/errors-and-security/) y [cómo probar los endpoints](/guides/testing/).

## Qué no incluye todavía

El CLI experimental genera una aplicación de servidor y archivos base. El scaffold incorpora un pool SQLx, un endpoint `/health/database` y un binario independiente de migraciones; consulta [Base de datos](/guides/database/). RustFlow también ofrece APIs ORM SQL experimentales mediante SeaORM y un ODM MongoDB separado, pero el CLI no genera automáticamente entidades SeaORM, modelos `DocumentModel` ni una aplicación CRUD integrada. El scaffold tampoco conecta automáticamente las primitivas de sesión, los guards de autenticación/autorización ni el middleware CSRF. Esas APIs experimentales están disponibles por separado: consulta [formularios y validación](/guides/forms-validation/) y [sesiones y control de acceso](/guides/sessions-and-access/). La aplicación debe verificar credenciales, asignar identidad/roles desde datos confiables y conectar CSRF deliberadamente a las rutas mutables que usan cookies. Siguen pendientes server actions, proveedor de usuarios y almacenamiento SQL durable para sesiones; `InMemorySessionStore` permanece local al proceso. Los forms mejorados del navegador están limitados a ciertos GET URL-encoded; no son un sistema de mutaciones server-side. El router HTTP y el callback de navegación cliente son responsabilidades separadas, no un router full-stack integrado.
