---
title: Servidor HTTP experimental
description: Ejecuta el prototipo de servidor y explora rutas, JSON, documentos HTML y páginas de error.
---

> **Estado: prototipo pre-1.0.** Este crate permite experimentar con routing y respuestas HTTP, pero no es una receta de despliegue de producción ni forma todavía un framework full-stack completo.

## Ejecutar el ejemplo

Clona el repositorio, entra en el directorio y ejecuta desde la raíz:

~~~sh
cargo run -p rustflow-server --example basic
~~~

El ejemplo escucha en `127.0.0.1:3000` y expone rutas para una página HTML, estado de salud JSON, parámetros de usuario y una operación de eco JSON. La dirección es local; no la interpretes como una configuración de despliegue recomendada.

## Capacidades del prototipo

- Registro de rutas y handlers asíncronos, parámetros de ruta y decodificación tipada de query/JSON.
- Respuestas de texto, HTML, JSON, redirección, descarga y códigos de estado.
- Middleware global ordenado, rutas nombradas e introspección de rutas.
- Límites configurables de body, timeout de lectura/handler y límite de concurrencia.
- Correlación mediante `Request::request_id()` y `x-request-id`; el servidor emite spans/eventos `tracing`, pero el binario de la aplicación debe configurar su propio subscriber/exporter.
- API de apagado graceful que recibe una future desde el punto de composición de la aplicación.

Consulta la [guía técnica HTTP](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md) para ejemplos de registro, configuración y límites.

## HTML seguro y documentos completos

El crate `rustflow-ssr` serializa árboles de vista estructurales a fragmentos HTML escapados. El servidor ofrece `html_document(title, lang, &view)` y `html_document_with_components` para producir un documento con doctype, idioma, metadatos UTF-8, título y cuerpo.

`Response::html(String)` inserta la cadena proporcionada sin escapar. Úsala solo con contenido confiable ya escapado; para vistas estructurales, prefiere `html_document` o los helpers de fragmento.

## Páginas de error HTML configurables

Puedes configurar títulos y mensajes para estados 4xx/5xx. Se renderizan como texto, no como HTML sin procesar:

~~~rust
use http::StatusCode;
use rustflow_server::{Application, ErrorPages, ServerConfig};

fn configure_app() -> Result<Application, rustflow_server::ErrorPageConfigError> {
    let error_pages = ErrorPages::new()
        .with_page(
            StatusCode::NOT_FOUND,
            "No encontrado",
            "La página no existe.",
        )?
        .with_page(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error del servidor",
            "No se pudo completar la solicitud.",
        )?;

    let config = ServerConfig::new().with_error_pages(error_pages);
    Ok(Application::with_config(config))
}
~~~

Este es un fragmento de configuración; un binario debe registrar sus rutas y llamar a la API de servidor que corresponda. Solo se aceptan estados de error. Los estados no configurados mantienen su respuesta habitual; las páginas personalizadas conservan el status y el `x-request-id`.

## Pruebas de integración por TCP local

La suite de integración puede ejecutarse con:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

Las pruebas usan un listener TCP local con puerto efímero y cubren rutas HTML/JSON, path/query, cabeceras de correlación, páginas personalizadas 404/405, query inválida 400, límite de body 413, timeout 504 y apagado graceful. Las operaciones de conexión/escritura/lectura del helper tienen timeouts para evitar que una regresión deje una prueba esperando indefinidamente.

Esta cobertura es representativa, no exhaustiva. No demuestra compatibilidad TLS o reverse proxy, confianza en cabeceras de proxy, autenticación/CSRF, resistencia a carga, manejo de abortos de cliente ni robustez frente a todas las entradas HTTP malformadas.

## Límites actuales

- No hay CLI ni scaffolding de proyectos.
- No hay sesiones, autenticación, autorización ni persistencia integradas.
- No hay bundle de navegador, hidratación ni navegación cliente.
- Layouts avanzados, grupos de rutas y middleware por ruta siguen pendientes.
- Las APIs son inestables y RustFlow no se considera listo para producción.

Consulta el [estado del proyecto](/project-status/) y el [roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) antes de depender de una capacidad.
