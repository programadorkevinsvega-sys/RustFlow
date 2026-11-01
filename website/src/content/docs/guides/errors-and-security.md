---
title: Errores, límites y seguridad HTTP
description: Maneja errores, contenido HTML, límites de petición, TLS y proxies sin asumir garantías que RustFlow no ofrece.
---

Esta guía reúne decisiones importantes para usar el adaptador HTTP actual sin confundir los controles disponibles con una configuración de producción completa. RustFlow sigue siendo pre-1.0: la aplicación debe validar datos de negocio, decidir los permisos y probar su despliegue.

## 1. Convertir errores de entrada en respuestas HTTP

Los handlers pueden devolver `HttpError` para rechazar una entrada o expresar un fallo sin construir manualmente una respuesta. Los helpers visibles incluyen `bad_request`, `not_found` e `internal_server_error`.

~~~rust
use rustflow_server::HttpError;

fn require_slug(slug: &str) -> Result<(), HttpError> {
    if slug.is_empty() {
        return Err(HttpError::bad_request("El slug es obligatorio"));
    }
    Ok(())
}
~~~

Este fragmento muestra únicamente el contrato de error; conecta la función a tu handler real. Los mensajes de error son parte de la respuesta pública: no incluyas consultas SQL, trazas, rutas privadas del sistema de archivos, tokens ni credenciales. Para fallos inesperados, usa el error 500 genérico en lugar de exponer el error interno.

Si necesitas un mensaje de error personalizado, asegúrate de que se pueda mostrar a cualquier usuario que reciba ese estado. No uses el estado de respuesta como sustituto de validación o autorización de negocio.

## 2. HTML seguro: vistas estructurales antes que strings

Para páginas, prefiere `View`, `html_view`, `html_document` o `HtmlPage`. El serializador SSR escapa texto y atributos admitidos y rechaza clases de markup/atributos que no están en su allowlist.

**Importante:** `Response::html(String)` emite la cadena suministrada sin escape. No pases entrada del usuario ni HTML generado por interpolación directa. El escape de SSR tampoco valida permisos ni convierte un dato confidencial en un dato público.

~~~
let body = View::element("p", [View::text(user_supplied_name)])?;
html_document("Perfil", "es", &body)
~~~

El bloque anterior ilustra el patrón estructural, no un handler completo: la creación de `body` debe estar dentro de un contexto que gestione el error `ViewError` y la respuesta HTTP.

Para páginas 4xx/5xx se puede configurar `ErrorPages`. El título y mensaje visibles se renderizan como texto, no como HTML. Elige mensajes útiles pero genéricos y deja el detalle técnico en logs correlacionados mediante el request ID.

## 3. Limitar el trabajo de cada petición

`ServerConfig` permite configurar el tamaño máximo del cuerpo recibido, el timeout de lectura/ejecución de handler y el máximo de peticiones concurrentes. Usa límites coherentes con los payloads reales de tu aplicación; no uses límites muy grandes como sustituto de streaming, porque el adaptador almacena el cuerpo recibido.

Los límites del servidor y de `HydrationData` son controles de recursos distintos. El límite de JSON para hidratación no sustituye el tamaño máximo del cuerpo HTTP, la validación de campos ni el límite de datos de negocio.

El adaptador genera o valida un `x-request-id` y lo expone a los handlers para correlación de logs. **No lo uses como credencial, prueba de identidad ni autorización.** `tracing` emite eventos/spans con request ID, método, plantilla de ruta, status y duración. El adapter no registra headers, cookies, autorización, query values ni cuerpos. La aplicación configura su subscriber/exporter y debe mantener su propia política de redacción.

## 4. Reportes internos sin filtrar secretos

La aplicación puede registrar un <code>ErrorReporter</code> para recibir errores HTTP internos 5xx. El objeto <code>ErrorReport</code> solo ofrece request ID, método, plantilla de ruta, status y una categoría segura (<code>handler</code>, <code>middleware</code> o <code>request_timeout</code>). No incluye el mensaje de error, stack trace, headers, cookies, tokens, query values o body.

El reporter es asíncrono, se ejecuta en una tarea aislada y tiene un límite de espera (250 ms predeterminados). Si falla, entra en panic o supera el límite, RustFlow registra un mensaje genérico y conserva la respuesta HTTP original. No agregues secretos ni texto de diagnóstico a los campos allowlisted ni crees tareas detached ilimitadas dentro del callback.

## 5. Liveness, readiness y drenaje

Los endpoints framework-owned se activan explícitamente para no colisionar con rutas existentes. Liveness no consulta dependencias externas. Readiness ejecuta solo los checks que la aplicación registró, concurrentemente y con timeout por check. Fallos, panics y timeouts retornan 503 con un cuerpo JSON fijo; no enumeran nombres de dependencias, hosts, credenciales ni mensajes del proveedor. No realices migraciones ni mutaciones desde estos probes.

El shutdown graceful también está limitado por un deadline configurable. Cuando vence, el servidor retorna <code>ServerError::ShutdownTimedOut</code> y cancela el trabajo restante. Esta cancelación puede interrumpir código de aplicación: diseña operaciones mutables para usar transacciones y evita depender de side effects externos no idempotentes.

## 6. TLS y proxies confiables

RustFlow ofrece configuración TLS con rustls y una política explícita para reconstruir la IP y el esquema originales cuando hay un reverse proxy. La política de proxies confiables debe enumerar las redes de los proxies que controlas. Las cabeceras reenviadas de una conexión no confiable no se deben tratar como identidad verificada.

Antes de aceptar encabezados `Forwarded` o `X-Forwarded-*`, verifica que el proxy elimine valores suministrados por el cliente y escriba él mismo los valores correctos. Protege también el enlace entre proxy y servidor de origen cuando la topología lo requiera.

La configuración TLS local ayuda a probar el transporte; no es una receta completa de certificados, renovación, DNS, firewall o reverse proxy. Consulta la API del crate `rustflow-server` y las pruebas de integración antes de adaptar la configuración.

## 7. Checklist antes de exponer un servicio

- Valida tipos, rangos, longitudes y reglas de negocio después de decodificar path, query o JSON.
- Comprueba identidad y autorización en la capa de aplicación que protege cada recurso.
- Usa respuestas HTML estructurales; reserva `Response::html` para una cadena que ya controlas y has preparado de forma segura.
- Configura límites de cuerpo, timeouts y concurrencia según el perfil de carga.
- No registres tokens, credenciales, cookies o cuerpos sensibles en logs.
- Define explícitamente la política de TLS y proxies confiables; prueba el escenario real de despliegue.
- Prueba respuestas malformadas, no autorizadas y de recurso inexistente con la suite HTTP.
- Revisa dependencias y configuración del entorno antes de publicar.

## Lo que RustFlow todavía no resuelve por ti

El adaptador ofrece APIs experimentales de extracción/validación tipada, middleware CSRF optativo basado en tokens firmados, sesiones server-side y guards de acceso. CSRF debe conectarse explícitamente a las mutaciones autenticadas mediante cookies; no sustituye una autorización de negocio. RustFlow también incorpora un ORM SQL experimental basado en SeaORM para SQLite/PostgreSQL/MySQL/MariaDB y un ODM MongoDB documental independiente; consulta la [guía de base de datos](/guides/database/) y la [guía ODM](/guides/mongodb-odm/). Sin embargo, no hay proveedor de usuarios/credenciales, rate limiting de aplicación, almacén SQL durable de sesiones, gestor de secretos ni server actions. El scaffold no genera una aplicación CRUD integrada automáticamente. Los controles de entrada, persistencia y transporte no convierten el proyecto en una plataforma lista para producción. Consulta las guías de [formularios y validación](/guides/forms-validation/) y [sesiones y control de acceso](/guides/sessions-and-access/).

## Referencias

- [Servidor HTTP](/guides/server/)
- [SSR y escape de HTML](/guides/ssr/)
- [Disponibilidad de API](/reference/api-status/)
- [Ejecutar y comprobar las pruebas](/guides/testing/)
- [Suite de integración HTTP](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/tests/http_integration.rs)
- [Contrato técnico HTTP](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md)
