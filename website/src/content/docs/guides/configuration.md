---
title: Configuración por entornos
description: Configura perfil, host y puerto en desarrollo, pruebas y producción con precedencia explícita y sin incluir secretos.
---

La configuración del servidor de una aplicación creada con el CLI se carga mediante `rustflow_server::AppConfig`. Es una API experimental que valida el perfil y la dirección de bind antes de que el servidor empiece a escuchar. `AppConfig` no configura bases de datos, sesiones ni autenticación y no gestiona secretos; las primitivas experimentales de sesión/guards son APIs separadas, documentadas en [Sesiones y control de acceso](/guides/sessions-and-access/).

## 1. Valores iniciales

El proyecto creado incluye .env.example. Es una referencia segura, no un archivo que se cargue por sí mismo.

~~~sh
cp .env.example .env
rustflow dev
~~~

El contenido inicial es deliberadamente pequeño:

~~~dotenv
RUSTFLOW_ENV=development
RUSTFLOW_HOST=127.0.0.1
RUSTFLOW_PORT=3000
~~~

El host usa una IP literal; el puerto debe estar entre 1 y 65535. Sin valores ni archivos personalizados, el servidor escucha en 127.0.0.1:3000.

## 2. Precedencia

En desarrollo y pruebas, la aplicación combina las fuentes de configuración en este orden (de mayor a menor prioridad):

1. Variables del proceso.
2. .env.<perfil>, como .env.test.
3. .env.
4. Valores predeterminados.

El perfil proviene de RUSTFLOW_ENV; si no se define en el proceso, se lee de .env, y si tampoco existe, el perfil es development. Por ejemplo, un ajuste en el proceso reemplaza el valor de cualquiera de los archivos:

~~~sh
RUSTFLOW_PORT=3100 rustflow dev
~~~

Los archivos se leen como datos; el cargador no exporta valores al entorno global, ejecuta comandos ni expande $VARIABLE. Las líneas malformadas provocan un error con el archivo y la línea que deben revisarse.

## 3. Desarrollo, test y producción

**Desarrollo.** Usa .env y opcionalmente .env.development para valores locales no sensibles.

**Pruebas.** Establece el perfil en el proceso para activar los ajustes de .env.test:

~~~sh
RUSTFLOW_ENV=test cargo test
~~~

**Producción.** No se leen .env, .env.production ni otros archivos dotenv. El perfil debe seleccionarse en el entorno del proceso y tanto host como puerto son obligatorios:

~~~sh
RUSTFLOW_ENV=production RUSTFLOW_HOST=0.0.0.0 RUSTFLOW_PORT=8080 ./target/release/hello_rustflow
~~~

Ajusta el nombre del binario al paquete de tu aplicación. En un contenedor o plataforma de despliegue, define estas variables en la configuración del servicio. Un perfil de producción incompleto, un host no numérico, un puerto inválido o un perfil desconocido detienen el arranque en lugar de recurrir silenciosamente a valores de desarrollo.

## 4. Secretos y archivos ignorados

El .gitignore del scaffold excluye .env y .env.*, con una excepción explícita para .env.example. No pongas contraseñas, tokens, claves privadas, credenciales reales o cadenas de conexión privadas en el ejemplo ni en el control de versiones. No publiques valores de producción en errores o logs.

Usa variables inyectadas por el entorno de ejecución o el almacén de secretos administrado de la plataforma, con acceso y rotación restringidos. RustFlow no incluye todavía un gestor de secretos ni necesita secretos para la configuración de host/puerto implementada en esta fase. Evita agregar variables para capacidades que aún no existen.

## 6. Nivel de logs y observabilidad

La aplicación puede resolver el nivel máximo de eventos mediante <code>TracingConfig</code> y la variable <code>RUSTFLOW_LOG_LEVEL</code>:

| Valor | Eventos permitidos |
|---|---|
| <code>error</code> | Solo errores |
| <code>warn</code> | Warnings y errores |
| <code>info</code> | Información, warnings y errores (predeterminado) |
| <code>debug</code> | Debug y niveles más severos |
| <code>trace</code> | Todos los niveles |

En desarrollo y pruebas, <code>RUSTFLOW_LOG_LEVEL</code> respeta la precedencia de <code>AppConfig</code>: variable del proceso, archivo <code>.env.&lt;perfil&gt;</code>, archivo <code>.env</code>, y valor por defecto. En producción solo se consulta el entorno del proceso. Los valores no soportados generan un error de configuración.

~~~rust
use rustflow_server::{AppConfig, TracingConfig};

let app_config = AppConfig::load()?;
let tracing_config = TracingConfig::from_app_config(&app_config)?;

// El binario de la aplicación configura su subscriber una sola vez.
// tracing_subscriber::fmt()
//     .with_max_level(tracing_config.tracing_level().into())
//     .try_init()?;
~~~

RustFlow ya emite spans/events estructurados para peticiones HTTP, incluidos request ID, método, plantilla de ruta y duración. El framework no instala un subscriber ni un exporter global. La aplicación elige su formatter, filtros por módulo y backend de exportación. No registres tokens, cookies, autorización, parámetros sensibles ni cuerpos de formularios.

## 7. Timeouts y endpoints operativos

<code>ServerConfig</code> permite cambiar límites del HTTP adapter sin confundirlos con los timeouts propios de pools SQL, cliente MongoDB o probes de readiness:

| Ajuste | Valor inicial |
|---|---:|
| Body HTTP máximo | 1 MiB |
| Timeout de lectura/handler | 30 s |
| Concurrencia HTTP | 256 |
| Drenaje máximo de graceful shutdown | 30 s |
| Espera máxima del reporter | 250 ms |

El timeout de shutdown se modifica con <code>ServerConfig::with_shutdown_timeout</code>. Al vencer, RustFlow cancela los futures de los handlers activos (también los que esperan un cupo de concurrencia), espera a que se liberen sus guardas de petición y devuelve <code>ServerError::ShutdownTimedOut</code>.

Los endpoints <code>/health/live</code> y <code>/health/ready</code> se registran explícitamente mediante <code>Application::enable_health_endpoints()</code> o <code>with_health_endpoints</code>. Readiness solo consulta los checks obligatorios que la aplicación registre mediante <code>readiness_check</code>; cada check tiene su propio timeout y los fallos se muestran como una respuesta genérica 503.

## 5. API disponible y límites

El scaffold usa este extracto desde su función main asíncrona, después de crear y configurar la aplicación:

~~~rust
let config = rustflow_server::AppConfig::load()?;
let address = config.socket_addr();
app.serve(address).await?;
~~~

AppConfig expone el perfil, la IP y el puerto validados. Los ajustes SQLx se resuelven por separado con `AppConfig::setting` y `DatabaseConfig::from_app_config`; consulta [Base de datos, SQLx y migraciones](/guides/database/). No hay formato TOML/YAML, resolución DNS, perfiles personalizados ni gestor de secretos. El framework sigue siendo pre-1.0 y no debe considerarse listo para producción solo por validar estas variables.

Para crear el scaffold desde cero, sigue el tutorial de primera aplicación. Para consultar comandos y generadores, usa la guía del CLI.
