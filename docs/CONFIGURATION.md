# Configuración por entornos (Fase 6.2)

La configuración de RustFlow sigue siendo experimental y deliberadamente pequeña. La API pública `rustflow_server::AppConfig` valida el perfil de ejecución y la dirección de bind del servidor. No configura bases de datos, sesiones, autenticación ni secretos de aplicación; estas capas se configuran por separado y las primitivas experimentales de sesiones/guards se describen en [HTTP_SERVER.md](HTTP_SERVER.md).

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

## Configuración de bases de datos (contrato separado)

Las variables de base de datos no forman parte de las tres variables de bind que valida `AppConfig`. Los adaptadores las leen mediante su propia configuración, manteniendo la precedencia existente: desarrollo/test usan proceso > `.env.<perfil>` > `.env`; producción exige valores del entorno del proceso.

### Motores SQL

`RUSTFLOW_DATABASE_URL` selecciona el driver SQLx. Esquemas admitidos:

- SQLite: `sqlite://rustflow.db?mode=rwc` o `sqlite::memory:`.
- PostgreSQL: `postgres://<user>:<password>@<host>/<database>` o `postgresql://...`.
- MySQL y MariaDB: `mysql://<user>:<password>@<host>/<database>` para el driver de protocolo MySQL de SQLx. `mariadb://` no es aceptado.

Los ajustes de pool opcionales son `RUSTFLOW_DATABASE_MAX_CONNECTIONS`, `RUSTFLOW_DATABASE_MIN_CONNECTIONS` y `RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS`. Las URLs son secretas si incluyen credenciales; no las copies en logs ni en archivos versionados. SQLx comparte el límite de pool/transacción, no una promesa de SQL portable. Placeholders, DDL, tipos y archivos de migración deben corresponder al motor real. El scaffold CLI es SQLite-first: cambiar la URL no porta las consultas ni las migraciones generadas.

### MongoDB documental (adaptador separado)

MongoDB no se selecciona mediante `RUSTFLOW_DATABASE_URL` y no usa SQLx, `AnyPool` ni migraciones SQL. Usa:

- `RUSTFLOW_MONGODB_URI=mongodb://<user>:<password>@<host>:27017/<database>?authSource=admin`
- `RUSTFLOW_MONGODB_DATABASE=<database>`
- Opcionales, en segundos positivos: `RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS`, `RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS`, `RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS`.

La implementación exige que la aplicación construya `MongoConfig` y `MongoDatabase` explícitamente; la plantilla generada no inicializa MongoDB. Solo URI estándar `mongodb://` está habilitada en esta configuración; `mongodb+srv://` no está soportada mientras el feature del resolver DNS permanezca deshabilitado. El bootstrap declarativo administra colecciones e índices, pero no es un sistema de migraciones de documentos. Consulta [DATABASES.md](DATABASES.md) y [DATABASE_SUPPORT_MATRIX.md](DATABASE_SUPPORT_MATRIX.md) para límites y versiones objetivo.

## Límites actuales

La configuración no es un sistema general de configuración TOML/YAML, no incluye perfiles personalizados ni resuelve nombres DNS. Los ajustes de base de datos de la Fase 6.5 usan `AppConfig::setting`: en desarrollo/test respetan proceso > `.env.<perfil>` > `.env`; en producción solo consultan el entorno del proceso. El framework no incluye un gestor de secretos. Las APIs son pre-1.0 y el contrato público puede cambiar. Validación de formularios, sesiones, autenticación/autorización y CSRF siguen siendo APIs separadas; la persistencia SQLx tiene su propio contrato en [DATABASES.md](DATABASES.md).

Consulta también la guía pública de configuración, docs/CLI.md y ROADMAP.md.
