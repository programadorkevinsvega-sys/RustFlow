---
title: "Tutorial: una API JSON"
description: Recorre los endpoints HTTP ejecutables de RustFlow para query strings, parámetros de ruta y cuerpos JSON.
---

Este tutorial usa el servidor real del repositorio, no una API inventada para la documentación. Puedes crear primero una app con el [CLI experimental](/guides/cli/), pero el endpoint JSON de este tutorial pertenece al ejemplo canónico y no se agrega automáticamente al scaffold. La implementación fuente es [`crates/rustflow-server/examples/basic.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs). Incluye endpoints de demostración y no depende de una base de datos.

## 1. Arrancar el servidor

Desde la raíz de RustFlow:

~~~sh
cargo run -p rustflow-server --example basic
~~~

El ejemplo escucha en `127.0.0.1:3000`. Déjalo ejecutando y usa otra terminal para hacer las peticiones siguientes.

## 2. Consultar un endpoint de estado

~~~sh
curl -i http://127.0.0.1:3000/api/health
~~~

El handler devuelve JSON con el estado `ok`. La ruta está registrada dentro de un grupo con prefijo `/api` y con nombre `api.health`, para ilustrar cómo agrupar e identificar endpoints relacionados.

## 3. Leer un parámetro de ruta y una query tipada

~~~sh
curl -i 'http://127.0.0.1:3000/users/42?detailed=true'
~~~

El handler lee `id` mediante `request.path_param("id")` y decodifica `detailed` a una estructura tipada con `request.query::<UserQuery>()`. La respuesta incluye el identificador y el valor booleano. Prueba también `?detailed=false` y omite el parámetro para observar el valor predeterminado definido por el ejemplo.

Los parámetros son datos de entrada; si tu aplicación los usa para recuperar recursos, debe validar el formato y comprobar permisos explícitamente. Que el path contenga un ID no demuestra que la persona que hace la petición pueda leer ese recurso.

## 4. Recibir y devolver JSON

~~~sh
curl -i -X POST http://127.0.0.1:3000/api/echo \
  -H 'content-type: application/json' \
  -d '{"message":"hola RustFlow"}'
~~~

El cuerpo se decodifica con `request.json::<EchoPayload>()` y la respuesta usa `Response::json`. La estructura `EchoPayload` del ejemplo solo contiene `message: String`; es un eco educativo, no validación de un modelo de negocio.

Prueba un JSON malformado para observar la respuesta de error del adapter:

~~~sh
curl -i -X POST http://127.0.0.1:3000/api/echo \
  -H 'content-type: application/json' \
  -d '{'
~~~

La decodificación detecta el cuerpo inválido, pero no valida automáticamente reglas como longitudes máximas, permiso para una operación o consistencia con almacenamiento. Añade esa validación en tu handler/capa de aplicación.

## 5. Qué responsabilidades muestra el ejemplo

| API | Responsabilidad |
|---|---|
| `Application::new` y `app.get/post` | Registra rutas y handlers asíncronos. |
| `app.group("/api", ...)` | Comparte prefijo y permite asignar prefijos de nombres de ruta. |
| `Request::path_param` | Recupera un parámetro declarado en el patrón de la ruta. |
| `Request::query::<T>()` | Decodifica la query string a un tipo Serde. |
| `Request::json::<T>()` | Decodifica el cuerpo JSON a un tipo Serde. |
| `Response::json` | Serializa el resultado y establece el tipo de contenido JSON. |
| `Middleware::new` | Intercepta la petición/respuesta o pasa al siguiente handler. |

El ejemplo completo es la fuente de verdad para los imports, tipos y firmas exactos de la revisión actual; consulta el archivo antes de copiar funciones a un proyecto nuevo.

## 6. Adaptar el patrón a una API real

1. Define tipos de entrada pequeños con los campos aceptados por cada endpoint.
2. Decodifica path, query o cuerpo JSON y responde con un error HTTP si la entrada no se puede interpretar.
3. Aplica reglas de negocio, validación de rangos y autorización en la capa que controla el recurso.
4. Devuelve tipos de respuesta explícitos con `Response::json`.
5. Añade tests para valores válidos, datos malformados, recurso inexistente y operaciones que deben denegarse.
6. No registres secretos ni cuerpos sensibles como parte del diagnóstico.

## Límites del ejemplo

La tabla y las respuestas del servidor básico son estáticas. RustFlow no proporciona una capa integrada de persistencia, ORM, migraciones, sesiones, autenticación/autorización, CSRF ni server actions. Añadir esos sistemas no consiste únicamente en cambiar el tipo pasado a `request.json`.

Para respuestas HTML seguras, composición SSR y layouts, continúa con [Servidor HTTP](/guides/server/), [SSR](/guides/ssr/) y el [tutorial de página de producto](/tutorials/product-page/). Para la lista completa de demostraciones, consulta el [catálogo de ejemplos](/reference/examples/).
