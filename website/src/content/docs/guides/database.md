---
title: Base de datos, SQLx y migraciones
description: Configura pools SQLite/PostgreSQL, ejecuta consultas parametrizadas, usa transacciones y aplica migraciones versionadas.
---

La integración SQLx de RustFlow es experimental y pre-1.0. Proporciona configuración de pool, transacciones explícitas, ejecución/reversión de migraciones y mapeo seguro de errores. No es un ORM: las entidades y las consultas siguen siendo código de la aplicación.

## 1. Configurar el pool

La PR #54 añadió SQLite, PostgreSQL y el driver de protocolo MySQL de SQLx mediante `AnyPool`; fue fusionada en `main` con el commit `48a491d660c71005b58ed88a4f6a7e8ba3390c9d`. Las URLs `mysql://` son compartidas por MySQL y MariaDB, y `DatabaseBackend::MySql` identifica el driver, no el producto servidor. La fase 6.6.2 introduce un adaptador documental MongoDB separado de SQLx. El contrato inicial de integración usa MongoDB 7.0.45 y debe verificarse con los checks del PR #55.

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

Para desarrollo local puede usarse `sqlite://rustflow.db?mode=rwc`. Para PostgreSQL, MySQL o MariaDB configura una URL válida desde el entorno local o el gestor de secretos del despliegue; nunca guardes credenciales reales en el repositorio. Las URLs de `DatabaseConfig` se omiten en su salida `Debug`. Para MySQL y MariaDB usa el esquema `mysql://`; `mariadb://` no está admitido.

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


### Migraciones específicas de MySQL/MariaDB

SQLx no convierte automáticamente DDL entre motores. Mantén migraciones separadas cuando difieran la sintaxis, los tipos o funciones específicas del proveedor. Cada directorio de migración de MySQL/MariaDB debe contener `.rustflow-dialect` con el valor `mysql`; RustFlow valida este marcador antes de aplicar o revertir migraciones. Los directorios SQLite/PostgreSQL existentes sin marcador siguen siendo compatibles.

Algunas operaciones DDL de MySQL-family hacen commits implícitos; no asumas atomicidad total para una migración fallida. Ensaya recuperación contra el servidor de destino y prefiere migraciones correctivas hacia delante en producción.

Para ejecutar la suite de integración local, configura dos bases desechables con `RUSTFLOW_TEST_MYSQL_URL` y `RUSTFLOW_TEST_MARIADB_URL`, y ejecuta `cargo test -p rustflow-server --test database_engines -- --test-threads=1`. Fuera de CI cada caso se omite cuando falta su URL; en CI, la ausencia es un error.

## Adaptador documental MongoDB (fase 6.6.2)

MongoDB no usa DatabasePool, SQLx AnyPool, SQL ni migraciones *.sql. RustFlow expone un límite documental separado con MongoConfig, MongoDatabase, BSON y los tipos nativos del driver oficial.

Configura RUSTFLOW_MONGODB_URI y RUSTFLOW_MONGODB_DATABASE. Los timeouts opcionales en segundos son RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS, RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS y RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS. Los defaults son 10 s, 10 s y 15 s. MongoConfig::from_app_config respeta la política de configuración de RustFlow: en producción los valores vienen del entorno del proceso, no de archivos dotenv. La URI se redacta en Debug.

~~~rust
use rustflow_server::{AppConfig, MongoConfig, MongoDatabase};

let app = AppConfig::load()?;
let config = MongoConfig::from_app_config(&app)?;
let database = MongoDatabase::connect(&config).await?;
database.ping().await?;
~~~

Las colecciones usan las operaciones y los filtros nativos de MongoDB. run_with_timeout permite aplicar el timeout de operación al future que entrega el driver:

~~~rust
use rustflow_server::{
    DocumentBootstrap, DocumentIndex, MongoDatabase, mongodb_driver::bson::{doc, Document},
};

let notes = database.collection::<Document>("notes");
database
    .run_with_timeout(notes.insert_one(doc! { "title": "A note", "metadata": { "pinned": false } }))
    .await?;

let bootstrap = DocumentBootstrap::new("notes")?
    .with_index(DocumentIndex::new("title_idx", doc! { "title": 1 }, false)?)?;
database.bootstrap(&bootstrap).await?;
~~~

DocumentBootstrap prepara una colección si falta, crea índices declarados (incluidos los únicos) y comprueba que los nombres estén presentes. Puede repetirse con declaraciones sin cambios; no borra colecciones o índices, ni sustituye un índice incompatible, y no es un sistema de migraciones documentales. Los errores de BSON y del driver se conservan como fuentes para diagnóstico confiable, pero Display, Debug y los mensajes HTTP no imprimen URI ni mensajes internos sensibles. Solo el código de MongoDB 11000 se trata como conflicto HTTP 409.

La API es experimental. Esta fase no implementa ODM, derives de modelos, relaciones SQL, un query builder portátil ni migraciones/transacciones propias. La implementación y el contrato inicial de CI para MongoDB 7.0.45 están en PR #55; la matriz completa de motores pertenece a la subfase 6.6.3.

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

Esta integración SQLx no introduce ORM, modelos ActiveRecord, repositorio genérico ni generación de modelos SQL. MongoDB cuenta con un adaptador documental separado en la subfase 6.6.2; la matriz consolidada de compatibilidad de la subfase 6.6.3 continúa pendiente. Las APIs de base de datos son experimentales; sesiones y autorización siguen siendo contratos separados. InMemorySessionStore continúa siendo volátil y local al proceso.

## Roadmap de persistencia (planificado, no disponible)

- **Fase 6.6.1:** integrada en `main` desde PR #54; la matriz validada sigue acotada a MySQL 8.0/MariaDB 11.4. **Fase 6.6.2:** adaptador documental MongoDB en PR #55, con verificación de CI obligatoria antes de aceptar el contrato. **Fase 6.6.3:** matriz de compatibilidad consolidada pendiente.
- **Fase 6.7:** crear un ORM SQL maduro para SQLite, PostgreSQL, MySQL y MariaDB. MongoDB tendrá una subfase separada de mapeo documental/ODM maduro; no se presentará como SQL ni se forzarán relaciones SQL sobre colecciones de documentos. La fase no se cerrará con un ORM mínimo de solo CRUD: el diseño, consultas tipadas, relaciones/carga eficiente, transacciones, mapeo de tipos, diagnósticos y CI deben satisfacer el contrato completo aceptado antes de declarar el hito terminado.

Consulta el [roadmap completo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para criterios de aceptación. Estos hitos planificados no describen capacidades disponibles en la rama actual.

Para el alcance completo, consulta la documentación técnica de SQLx y migraciones (docs/DATABASES.md), la referencia de disponibilidad de API (/reference/api-status/) y el roadmap (ROADMAP.md).
