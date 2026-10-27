---
title: Servidor HTTP
description: Construye rutas y handlers y comprende las capacidades actuales del adapter HTTP.
---

El crate <code>rustflow-server</code> integra Axum, Tokio y Tower detrás de tipos propios de RustFlow. Su objetivo es ofrecer un punto de composición de peticiones HTTP con rutas, middleware y respuestas controladas por la aplicación. Es un **prototipo pre-1.0**, no una plataforma de despliegue lista para producción.

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

La configuración del servidor establece límites de body, timeouts para lectura/handler y un máximo de peticiones concurrentes. El servidor genera o valida un ID de petición, lo expone al handler y lo devuelve en <code>x-request-id</code>. Los eventos/spans de <code>tracing</code> se emiten desde el adapter, pero el binario de la aplicación configura su propio subscriber/exporter.

El API de apagado graceful recibe una future desde el punto de composición de la aplicación; la integración con señales del sistema operativo corresponde al caller. TLS local está implementado con rustls. La política de proxies de confianza requiere que el proxy sanee las cabeceras reenviadas y que el enlace hacia el origen esté protegido.

Estos controles son una base de ingeniería, no reemplazan rate limiting, revisión de amenazas, autenticación, autorización, CSRF, hardening del reverse proxy ni una receta de producción.

## 7. SSR, páginas y layouts

El servidor también integra:

- <code>rustflow_ssr</code> para serializar árboles de vistas como fragmentos HTML escapados.
- <code>html_document(title, lang, view)</code> para respuestas de documento completo.
- <code>HtmlPage</code> para página con título/idioma, componente de cuerpo, catálogo y layouts.
- <code>SharedComponentCatalog</code> para compartir definiciones inmutables entre peticiones concurrentes, con runtime y estado local por render.
- <code>HydrationData</code> y marcado opt-in para páginas que se preparan para hidratación.
- APIs de navegador separadas para hidratación y navegación cliente.

La hidratación y la navegación existen como capacidades explícitas y optativas; no se activan automáticamente en cada handler. Consulta [SSR y documentos HTML](/guides/ssr/) y [Composición de layouts](/guides/layouts/) para ver ejemplos de uso y los límites de cada mecanismo.

## 8. Verificar los endpoints

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

La suite cubre rutas por TCP local, query/path/JSON, grupos, middleware, IDs de petición, error pages, límites, timeouts, apagado graceful, TLS local, proxies de confianza, abortos de cliente, entradas malformadas y concurrencia acotada. Ejecuta también la suite completa del workspace antes de depender de un cambio.

## Qué no incluye todavía

No hay CLI/scaffolding, sesiones, autenticación/autorización integradas, CSRF, server actions, persistencia integrada ni una abstracción de aplicación lista para producción. Los forms mejorados del navegador están limitados a ciertos GET URL-encoded; no son un sistema de mutaciones server-side. El router HTTP y el callback de navegación cliente son responsabilidades separadas, no un router full-stack integrado.
