---
title: Base de datos, SeaORM, SQLx y migraciones
description: Configura SeaORM sobre pools SQLx tipados compartidos, consultas parametrizadas, transacciones y migraciones; MongoDB conserva su adaptador documental separado.
---

La capa de base de datos de RustFlow continúa siendo experimental y pre-1.0. Phase 6.7.1 seleccionó **Data Mapper con SeaORM 1.1.20** y añadió el derive de validación <code>rustflow::ModelDiagnostics</code>. La implementación de Phase 6.7.2 conecta SeaORM en <code>rustflow-server</code> y sustituye el antiguo <code>AnyPool</code> por un pool SQLx tipado por backend.

SeaORM se construye a partir de un clone del mismo pool SQLx; no se crea una segunda pool para la misma URL. La integración relacional cubre SQLite, PostgreSQL, MySQL y MariaDB. MongoDB conserva su adaptador documental independiente, con BSON, colecciones e índices; su ODM pertenece a la subfase 6.7.3.

La subfase 6.7.2 sigue en curso. El puente de conexión y el acceso a transacciones compartidas están en la rama de implementación, pero no representan el cierre del contrato ORM. CRUD completo, query builder, relaciones y loaders, codecs por motor, pruebas de N+1, matriz de tipos y gates CI siguen siendo criterios de aceptación pendientes.

## Foundation seleccionado para SQL ORM

RustFlow usa el patrón **Data Mapper** con SeaORM. Las entidades y modelos los genera SeaORM mediante <code>DeriveEntityModel</code>; <code>ModelDiagnostics</code> valida el subconjunto de metadatos de RustFlow, pero no reemplaza los derives de SeaORM.

~~~rust
use rustflow::ModelDiagnostics;
use rustflow_server::sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
#[sea_orm(table_name = "user_profiles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub display_name: String,
    pub nickname: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
~~~

El ejemplo representa el contrato de declaración de entidad. Para operaciones persistentes, pasa <code>database.orm()</code> o la transacción explícita a la API SeaORM. No agregues conexiones globales ni relaciones con carga perezosa oculta.

El SQL ORM cubre SQLite, PostgreSQL, MySQL y MariaDB. MongoDB sigue siendo el adaptador documental BSON separado y no comparte derives, relaciones SQL ni query builder relacional.

## Tipos numéricos y portabilidad

No todos los tipos numéricos de Rust tienen un codec común en los cuatro motores. En particular, SQLx no codifica directamente `u64` para SQLite ni PostgreSQL, incluso cuando el valor es pequeño y cabe en `i64`. MySQL y MariaDB sí admiten el tipo mediante `BIGINT UNSIGNED`; el contrato de integración verifica allí el round-trip de `u64::MAX`.

Para modelos compartidos entre los cuatro motores, usa `i64` cuando el dominio cabe en el rango firmado. No conviertas `u64` a `i64` mediante un cast sin comprobar el rango. El uso de `u64` queda limitado a modelos específicos de MySQL/MariaDB; una representación portable para todo el rango unsigned requiere un codec explícito y todavía no forma parte del contrato común.

Hay otra diferencia importante con los decimales: SeaORM 1.1.20/SeaQuery enlaza y recupera `Decimal` y `BigDecimal` en SQLite mediante `f64`. Por eso el esquema de prueba usa columnas `REAL` y limita el round-trip SQLite a `12.5`, que sí es exactamente representable en binario. Otros valores decimales pueden redondearse o expandirse al reconstruirse desde `f64`. No uses esos tipos en SQLite para dinero ni para cálculos que exijan precisión decimal arbitraria. Para ello usa enteros escalados o un codec de texto canónico diseñado expresamente. PostgreSQL usa `NUMERIC` y MySQL/MariaDB usan `DECIMAL`; las pruebas de integración verifican allí un `BigDecimal` de alta precisión.

## 1. Configurar el pool

La capa SQL usa SQLx 0.8.6 con un pool tipado por backend y SeaORM 1.1.20 sobre el mismo pool. MySQL y MariaDB comparten el driver `mysql`, así que `DatabaseBackend::MySql` identifica la familia del driver, no el producto servidor. MongoDB conserva un adaptador independiente de SQL. Consulta la [matriz técnica de compatibilidad](/guides/database/#matriz-de-versiones-verificadas) y el [contrato detallado de versiones](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/DATABASE_SUPPORT_MATRIX.md).

## Matriz de versiones verificadas

Estas son las versiones objetivo exactas que CI valida; no implican compatibilidad idéntica con todas las versiones de cada familia. La matriz se prueba en Rust 1.85 (MSRV del workspace) y Rust estable.

| Motor/servidor | Driver bloqueado | URI de conexión | Contrato comprobado en CI |
|---|---|---|---|
| SQLite 3.46.0 (incluido en el build) | SQLx 0.8.6; `sqlx-sqlite` / `libsqlite3-sys` 0.30.1 | `sqlite:` | Migraciones repetibles y reversibles, CRUD, transacciones y scaffold generado |
| PostgreSQL 16.15 | SQLx 0.8.6 | `postgres://`, `postgresql://` | Versión de servidor exacta, pool y rollback de transacción |
| MySQL 8.4.13 LTS | SQLx 0.8.6, driver `mysql` | `mysql://` | Versión exacta, CRUD, binding, transacciones, unicidad y migraciones |
| MariaDB 11.4.13 LTS | SQLx 0.8.6, mismo driver `mysql` | `mysql://` | Identidad y versión exactas de MariaDB, además del contrato SQL |
| MongoDB 7.0.45 | Driver oficial `mongodb` 3.6.0 (MSRV 1.85) | `mongodb://` | CRUD BSON, ObjectId, índices únicos, bootstrap, errores y timeouts acotados |

El scaffold del CLI continúa siendo SQLite-first. Cambiar `RUSTFLOW_DATABASE_URL` a otro motor SQL no traduce sus placeholders, tipos ni migraciones. MongoDB usa las variables `RUSTFLOW_MONGODB_URI` / `RUSTFLOW_MONGODB_DATABASE` y una API separada; no se inicializa automáticamente por la plantilla. `mongodb+srv://` no está habilitado en esta configuración porque el feature del resolver DNS está desactivado tras la revisión de seguridad de dependencias.



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

## 2. Acceso a SeaORM y SQLx tipado

`DatabasePool::orm()` devuelve la conexión SeaORM compartida con el pool SQLx. Usa esa referencia para Entity, ActiveModel, QuerySelect y loaders; las operaciones reciben explícitamente la conexión.

~~~rust
use rustflow_server::{AppConfig, DatabaseConfig, DatabasePool};

let app = AppConfig::load()?;
let config = DatabaseConfig::from_app_config(&app)?;
let database = DatabasePool::connect(&config).await?;
let orm_connection = database.orm();
~~~

Para SQLx directo, selecciona el accessor que corresponde al backend configurado. No existe un accessor genérico que convierta todos los motores en un mismo pool.

~~~rust
use rustflow_server::{DatabaseConfig, DatabasePool, sqlx};

let config = DatabaseConfig::new("sqlite://rustflow.db?mode=rwc")?;
let database = DatabasePool::connect(&config).await?;
let pool = database.sqlx_sqlite_pool().expect("configured SQLite backend");
let row = sqlx::query("SELECT id, title FROM notes WHERE id = ?")
    .bind(requested_id)
    .fetch_one(pool)
    .await?;
~~~

Los accessors disponibles son `sqlx_sqlite_pool()`, `sqlx_postgres_pool()` y `sqlx_mysql_pool()`; el último se utiliza tanto con MySQL como con MariaDB. El SQL y los placeholders siguen siendo específicos del motor: SQLite y MySQL/MariaDB suelen usar `?`, mientras PostgreSQL usa `$1`, `$2`, etc. No uses `format!` para interpolar datos del usuario. Para SQL parametrizado a través de SeaORM también existen `query_raw` y `execute_raw`.

## 3. Transacciones compartidas

`begin_transaction()` inicia una transacción SeaORM sobre el mismo pool. El acceso `transaction.connection()` implementa `ConnectionTrait`; así, las operaciones ORM y las sentencias SQL parametrizadas pueden participar en la misma transacción.

~~~rust
let transaction = database.begin_transaction().await?;
transaction
    .execute_raw(
        "INSERT INTO notes (title) VALUES (?)",
        [validated_title.into()],
    )
    .await?;
transaction.commit().await?;
~~~

Llama `rollback().await?` si la operación debe descartarse. Mantén las transacciones cortas y no las conserves abiertas durante llamadas HTTP u otras operaciones ajenas a la base de datos. Si se pierde la conexión mientras se confirma, el resultado del commit puede ser desconocido; no afirmes que el servidor realizó rollback.

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

MongoDB no usa DatabasePool, los pools SQLx tipados, SQL ni migraciones *.sql. RustFlow expone un límite documental separado con MongoConfig, MongoDatabase, BSON y los tipos nativos del driver oficial.

Configura RUSTFLOW_MONGODB_URI (URI estándar mongodb://; URI SRV mongodb+srv:// no habilitada en esta versión por la revisión de seguridad de la dependencia DNS) y RUSTFLOW_MONGODB_DATABASE. Los timeouts opcionales en segundos son RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS, RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS y RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS. Los defaults son 10 s, 10 s y 15 s. MongoConfig::from_app_config respeta la política de configuración de RustFlow: en producción los valores vienen del entorno del proceso, no de archivos dotenv. La URI se redacta en Debug.

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

La API es experimental. Esta fase no implementa ODM, derives de modelos, relaciones SQL, un query builder portátil ni migraciones/transacciones propias. El adaptador MongoDB y su contrato inicial de CI para MongoDB 7.0.45 se fusionaron mediante PR #55; los checks de CI y los gates de calidad pasaron en el HEAD final. La matriz completa de motores pertenece a la subfase 6.6.3.

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

El puente de conexión SeaORM/SQLx tipado y las transacciones compartidas ya están en implementación, pero Phase 6.7.2 todavía no está cerrada. Aún deben implementarse y verificarse CRUD/bulk, composición avanzada de consultas, relaciones y loaders explícitos, mapeo de tipos y NULL, pruebas de precisión/rangos en los cuatro motores, carga sin N+1 y evidencia de errores/commit ambiguo. MongoDB conserva su adaptador documental de la subfase 6.6.2; su ODM pertenece a Phase 6.7.3. Las APIs de base de datos siguen siendo experimentales; sesiones y autorización siguen siendo contratos separados. `InMemorySessionStore` continúa siendo volátil y local al proceso.

## Roadmap de persistencia

- **Phase 6.7.1:** cerrada y fusionada en PR #58; selecciona SeaORM 1.1.20 y define el contrato Data Mapper/ModelDiagnostics.
- **Phase 6.7.2:** en implementación. El puente de pool tipado, la fachada SeaORM y las transacciones compartidas están en desarrollo; CRUD/query builder, relaciones, codecs, pruebas por motor y gates completos siguen pendientes.
- **Phase 6.7.3 y posteriores:** ODM MongoDB separado, relaciones/mutaciones ampliadas, CLI/generated apps y quality gate. No se incluyen en esta subfase.

Consulta el [roadmap completo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para criterios de aceptación. La Phase 6.6.2 ofrece un adaptador experimental disponible en `main`; la matriz consolidada de la fase 6.6 y la API ORM/ODM prevista en la fase 6.7 siguen pendientes.

Para el alcance completo, consulta la documentación técnica de SQLx y migraciones (docs/DATABASES.md), la referencia de disponibilidad de API (/reference/api-status/) y el roadmap (ROADMAP.md).
