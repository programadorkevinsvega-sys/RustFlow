---
title: Base de datos, SQLx y migraciones
description: Configura pools SQLite/PostgreSQL, ejecuta consultas parametrizadas, usa transacciones y aplica migraciones versionadas.
---

La integración SQLx de RustFlow es experimental y pre-1.0. Proporciona configuración de pool, transacciones explícitas, ejecución/reversión de migraciones y mapeo seguro de errores. No es un ORM: las entidades y las consultas siguen siendo código de la aplicación.

## 1. Configurar el pool

Los únicos motores habilitados actualmente son SQLite y PostgreSQL. MySQL, MariaDB y MongoDB no están soportados por el código actual. El esquema de URL selecciona el driver SQLx para SQLite/PostgreSQL; MongoDB necesitará un adaptador independiente porque trabaja con documentos BSON, no SQL.

~~~rust
use rustflow_server::{AppConfig, DatabaseConfig, DatabasePool};

let app_config = AppConfig::load()?;
let database_config = DatabaseConfig::from_app_config(&app_config)?;
let database = DatabasePool::connect(&database_config).await?;
~~~

DatabaseConfig::from_app_config respeta la precedencia de configuración del servidor. En desarrollo/pruebas, RUSTFLOW_DATABASE_URL puede venir de .env, .env.<perfil> o del entorno del proceso. En producción, la URL debe provenir del entorno del proceso; los archivos dotenv se ignoran.

Configuración opcional del pool:

| Variable | Predeterminado | Propósito |
|---|---:|---|
| RUSTFLOW_DATABASE_MAX_CONNECTIONS | 10 | Límite superior de conexiones por proceso |
| RUSTFLOW_DATABASE_MIN_CONNECTIONS | 0 | Conexiones retenidas como mínimo |
| RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS | 10 | Tiempo máximo esperando una conexión |

Para desarrollo local puede usarse sqlite://rustflow.db?mode=rwc. Para PostgreSQL, configura un URL de conexión válido en el entorno local o en el gestor de secretos del despliegue; nunca guardes credenciales reales en el repositorio. RustFlow evita incluir el URL en la salida Debug de DatabaseConfig.

## 2. Consultas parametrizadas

La pool expone el AnyPool de SQLx mediante database.pool(). Puedes utilizar rustflow_server::sqlx sin añadir otra dependencia directa en el scaffold generado.

El siguiente ejemplo usa sintaxis de placeholders SQLite. El SQL puede variar entre motores, pero los valores recibidos del usuario deben enviarse como parámetros:

~~~rust
use rustflow_server::{DatabaseConfig, DatabasePool, sqlx};

let config = DatabaseConfig::new("sqlite://rustflow.db?mode=rwc")?
    .with_max_connections(8)?;
let database = DatabasePool::connect(&config).await?;

let row = sqlx::query("SELECT id, title FROM notes WHERE id = ?")
    .bind(requested_id)
    .fetch_one(database.pool())
    .await?;
~~~

No uses format! para insertar nombres, emails, IDs o texto del usuario dentro de una sentencia SQL. Los nombres de tabla/columna no suelen ser valores bindables; si necesitas elegirlos, usa una lista permitida definida por la aplicación.

## 3. Transacciones

begin_transaction devuelve una transacción explícita. Ejecuta las consultas a través de transaction.connection() y confirma solo cuando la operación completa haya tenido éxito:

~~~rust
let mut transaction = database.begin_transaction().await?;
rustflow_server::sqlx::query("INSERT INTO notes (title) VALUES (?)")
    .bind(validated_title)
    .execute(transaction.connection())
    .await?;
transaction.commit().await?;
~~~

Llama transaction.rollback().await? si la operación debe descartarse. Una transacción abandonada solicita rollback al cerrarse. Mantén las transacciones cortas y no las conserves abiertas mientras esperas llamadas HTTP u otras operaciones que no sean de base de datos.

## 4. Migraciones versionadas

Las migraciones se descubren desde el directorio indicado y se registran en _sqlx_migrations; SQLx conserva versiones y checksums para detectar modificaciones a archivos ya aplicados.

~~~rust
database.run_migrations("./migrations").await?;
~~~

Formatos aceptados por SQLx:

- 0001_create_notes.sql: migración forward-only.
- 0002_add_status.up.sql y 0002_add_status.down.sql: migración reversible.

Las migraciones deben coincidir con el dialecto de la base de datos configurada. Mantén directorios separados si SQLite y PostgreSQL necesitan DDL diferente.

El rollback se llama explícitamente:

~~~rust
database.rollback_migrations_to("./migrations", 1).await?;
~~~

El target representa la versión a la que se desea volver. Solo se revierten migraciones con archivo .down.sql. En producción suele ser más seguro escribir una nueva migración correctiva hacia delante que revertir una modificación de esquema ya consumida por otras instancias. Haz una copia de seguridad y ensaya recuperación antes de operaciones destructivas.

La aplicación generada aplica migraciones automáticamente en desarrollo/test. En producción, ejecuta cargo run --bin migrate una vez en el paso de despliegue antes de actualizar múltiples réplicas.

## 5. Errores de base de datos

DatabaseError::to_http_error traduce fallos conocidos a mensajes públicos genéricos:

| Condición | HTTP |
|---|---:|
| Fila no encontrada | 404 |
| Violación reconocida de unicidad | 409 |
| Pool agotado o cerrado | 503 |
| Fallo de configuración, consulta o migración no clasificado | 500 |

No devuelvas el error del controlador SQLx al usuario: puede contener detalles del SQL, esquema o conexión. Los errores internos deben registrarse con una política segura de logging, sin credenciales ni valores sensibles.

## 6. Pool sizing y operación

El límite de conexiones se aplica por proceso, no por aplicación completa. Estima el uso total como máximo del pool × número de réplicas y reserva conexiones para migraciones, trabajadores y herramientas operativas. Configura límites compatibles con max_connections del servidor y mide saturación antes de incrementarlos; más conexiones no siempre significan mayor rendimiento.

Las migraciones de producción deben ejecutarse como un paso coordinado, antes del despliegue de nuevas versiones. No habilites migraciones destructivas automáticas en el arranque de cada réplica.

## Límites actuales

Esta fase no introduce ORM, modelos ActiveRecord, un repositorio genérico, generación de modelos SQL ni soporte MySQL/MariaDB o MongoDB. Las APIs de base de datos son experimentales; sesiones y autorización siguen siendo contratos separados. InMemorySessionStore continúa siendo volátil y local al proceso.

## Roadmap de persistencia (planificado, no disponible)

- **Fase 6.6:** añadir soporte verificado para MySQL y MariaDB como motores relacionales y un adaptador independiente para MongoDB. La implementación, dependencias, configuración y pruebas aún están pendientes.
- **Fase 6.7:** crear un ORM SQL tipado para SQLite, PostgreSQL, MySQL y MariaDB. MongoDB tendrá una subfase separada de mapeo documental/ODM; no se presentará como SQL ni se forzarán relaciones SQL sobre colecciones de documentos.

Consulta el [roadmap completo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para criterios de aceptación. Estos hitos planificados no describen capacidades disponibles en la rama actual.

Para el alcance completo, consulta la documentación técnica de SQLx y migraciones (docs/DATABASES.md), la referencia de disponibilidad de API (/reference/api-status/) y el roadmap (ROADMAP.md).
