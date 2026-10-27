# Configuración por entornos (Fase 6.2)

La configuración de RustFlow sigue siendo experimental y deliberadamente pequeña. La API pública rustflow_server::AppConfig valida el perfil de ejecución y la dirección de bind del servidor. No configura todavía bases de datos, sesiones, autenticación ni secretos de aplicación.

## Archivos y precedencia

El cargador lee archivos del directorio de trabajo actual. Para development y test, la precedencia es:

1. Variables del proceso, que siempre ganan.
2. .env.<perfil>, por ejemplo .env.test.
3. .env.
4. Valores por defecto: host 127.0.0.1 y puerto 3000; el perfil por defecto es development.

.env.example es solo una plantilla; no se carga automáticamente. Cópiala si deseas personalizar tu entorno:

~~~sh
cp .env.example .env
rustflow dev
~~~

Los archivos dotenv se interpretan como datos y no modifican el entorno global del proceso. La sintaxis admite líneas KEY=value, comentarios, el prefijo opcional export y valores rodeados por comillas simples o dobles. No se realiza interpolación de variables ni expansión de comandos.

## Variables admitidas

| Variable | Desarrollo / test | Producción |
|---|---|---|
| RUSTFLOW_ENV | development, test o production; por defecto development | Debe seleccionarse explícitamente en el entorno del proceso como production |
| RUSTFLOW_HOST | IP numérica; por defecto 127.0.0.1 | Obligatoria en el entorno del proceso |
| RUSTFLOW_PORT | Entero entre 1 y 65535; por defecto 3000 | Obligatoria en el entorno del proceso |

El host debe ser una dirección IPv4 o IPv6; los nombres DNS no se resuelven. El puerto cero y los valores fuera de rango se rechazan. Los errores indican qué ajuste debe corregirse y no incluyen valores configurados.

## Separación de entornos

- **Desarrollo:** usa los valores locales de .env y puede sobreescribirlos en .env.development.
- **Pruebas:** selecciona RUSTFLOW_ENV=test y coloca solo ajustes no sensibles de pruebas en .env.test.
- **Producción:** proporciona RUSTFLOW_ENV=production, RUSTFLOW_HOST y RUSTFLOW_PORT desde el entorno del proceso, por ejemplo mediante la configuración del servicio de despliegue.

Producción no lee .env, .env.production ni otros archivos dotenv. Si el perfil de producción se declara solo dentro de un archivo dotenv, la aplicación falla en lugar de tratar ese archivo como configuración de despliegue. La configuración se valida antes de que el scaffold empiece a escuchar conexiones.

Ejemplo de arranque de producción en una shell:

~~~sh
RUSTFLOW_ENV=production RUSTFLOW_HOST=0.0.0.0 RUSTFLOW_PORT=8080 ./target/release/hello_rustflow
~~~

Adapta el nombre del ejecutable al nombre del paquete generado.

## Secretos

.env.example contiene únicamente valores no sensibles de host, puerto y perfil. No agregues contraseñas, tokens, claves privadas, cadenas de conexión privadas o datos reales a este archivo, a los tests, a los logs ni al control de versiones. El scaffold ignora .env y .env.* y mantiene explícitamente versionado solo .env.example.

Para despliegues, usa variables inyectadas por el entorno de ejecución o el almacén de secretos administrado de tu plataforma; limita el acceso y la rotación de acuerdo con esa plataforma. RustFlow todavía no incluye un gestor de secretos ni consume secretos de base de datos/sesión. No inventes valores secretos para satisfacer la configuración de esta fase.

## Uso desde Rust

El scaffold generado usa la configuración validada antes de arrancar el servidor:

~~~rust
let config = rustflow_server::AppConfig::load()?;
let address = config.socket_addr();
app.serve(address).await?;
~~~

Este es un extracto para insertar en el main asíncrono de una aplicación generada; app representa una instancia ya configurada de rustflow_server::Application.

## Límites actuales

La configuración no es un sistema general de configuración TOML/YAML, no incluye perfiles personalizados, no resuelve nombres DNS y no carga secretos. Las APIs son pre-1.0; el contrato público puede cambiar. La validación de formularios, sesiones, autenticación/autorización, CSRF y persistencia siguen siendo fases separadas.

Consulta también la guía pública de configuración, docs/CLI.md y ROADMAP.md.
