---
title: Base de datos, SeaORM, SQLx y migraciones
description: Configura SeaORM sobre pools SQLx tipados compartidos, consultas parametrizadas, transacciones y migraciones; MongoDB conserva su adaptador documental separado.
---

La capa de base de datos de RustFlow continúa siendo experimental y pre-1.0. Phase 6.7.1 seleccionó **Data Mapper con SeaORM 1.1.20** y añadió el derive de validación <code>rustflow::ModelDiagnostics</code>. Phase 6.7.2 incorporó SeaORM en <code>rustflow-server</code> sobre un único pool SQLx tipado por backend, compartido por SQLx y SeaORM. Phase 6.7.4 añadió la capa de carga relacional avanzada y quedó integrada mediante PR #62, squash merge <code>f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7</code>.

SeaORM se construye a partir de un clone del mismo pool SQLx; no se crea una segunda pool para la misma URL. La integración relacional cubre SQLite, PostgreSQL, MySQL y MariaDB. MongoDB conserva su adaptador documental independiente, con BSON, colecciones e índices. El ODM tipado de Phase 6.7.3 se implementó como una capa Data Mapper separada sobre el mismo cliente. PR #60 pasó los gates requeridos en su HEAD final `659f1750d611e57315263216167d43375da23574` y se fusionó en `main` el 2026-10-31 mediante squash (`7ffb89b5ca36017a428cf3f828b68f590f8d404b`); consulta la [guía ODM MongoDB](/guides/mongodb-odm/).

La subfase 6.7.2 quedó fusionada mediante Squash Merge en `main` el 2026-10-31, con el commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. <code>DatabasePool::entity</code> y <code>EntityRepository</code> ofrecen CRUD/bulk y queries SeaORM tipadas, loaders de relaciones explícitos y transacciones ORM/SQL compartidas. El contrato ejecutó SQLite, PostgreSQL, MySQL y MariaDB; también pasaron los límites numéricos por dialecto, los conteos de consultas anidadas, las mutaciones relacionales y la inyección real de pérdida de conexión durante <code>COMMIT</code>. Los 8 checks requeridos quedaron verdes en el SHA <code>2cadce73c19c37c2f088f572ea37a83c65c72380</code>. La API sigue siendo experimental; consulta los límites de codec y las fases pendientes descritos más abajo y en el contrato técnico.

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

La matriz de pruebas ampliada round-tripea los límites mínimo y máximo de `i8`, `i16`, `i32` e `i64` en SQLite, PostgreSQL, MySQL y MariaDB, y comprueba que los inserts fuera de rango sean rechazados por los tipos o restricciones de cada dialecto. `u8`, `u16` y `u32` se prueban en SQLite y MySQL/MariaDB, incluidos los límites inferior y superior; PostgreSQL no tiene mapeo para esos primitivos unsigned en SeaORM 1.1.20, por lo que no se anuncia como soportado. Para `u64`, solo se prueba el rango completo `BIGINT UNSIGNED` de MySQL/MariaDB, incluidos los valores negativos y superiores a `u64::MAX`.

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

Los modelos son entidades SeaORM de la aplicación; DatabasePool no convierte structs arbitrarios en tablas. El repositorio tipado enlaza una entidad al pool existente y ofrece CRUD/bulk sin abrir conexiones adicionales.

~~~rust
use rustflow_server::{
    DatabasePool,
    sea_orm::{
        ActiveValue::Set, ColumnTrait, QueryFilter, QueryOrder, QuerySelect,
    },
};

let accounts = database.entity::<entities::account::Entity>();

let account = accounts
    .insert(entities::account::ActiveModel {
        email: Set("ada@example.test".to_owned()),
        enabled: Set(true),
        alias: Set(Some("Ada".to_owned())),
        metadata: Set(serde_json::json!({"role": "writer"})),
        checksum: Set(vec![0, 1, 2]),
        ..Default::default()
    })
    .await?;

// La composición conserva el query builder tipado de SeaORM.
let _page = accounts
    .all(
        accounts
            .select()
            .filter(entities::account::Column::Enabled.eq(true))
            .order_by_asc(entities::account::Column::Id)
            .offset(20)
            .limit(20),
    )
    .await?;

// Bulk update y delete requieren un filtro explícito.
let changed = accounts
    .update_many()
    .col_expr(
        entities::account::Column::Enabled,
        rustflow_server::sea_orm::sea_query::Expr::val(false).into(),
    )
    .filter(entities::account::Column::Id.eq(account.id))
    .exec(database.orm())
    .await?;
assert_eq!(changed.rows_affected, 1);
~~~

### CRUD y operaciones masivas

`find_by_id` devuelve `Option<Model>` y recibe la clave primaria completa. `update` y `delete` operan sobre un `ActiveModel` con la clave definida; `save` inserta cuando la clave autogenerada está en `NotSet` y actualiza cuando ya está definida.

~~~rust
use rustflow_server::sea_orm::ActiveValue::Set;

// Leer por la clave primaria completa.
let current = accounts
    .find_by_id(account.id)
    .await?
    .expect("account exists");

// Actualizar una fila y devolver el modelo confirmado por la base de datos.
let updated = accounts
    .update(entities::account::ActiveModel {
        id: Set(current.id),
        alias: Set(Some("Ada Lovelace".to_owned())),
        ..Default::default()
    })
    .await?;

// save inserta si la clave autogenerada no está definida.
let _saved = accounts
    .save(entities::account::ActiveModel {
        email: Set("grace@example.test".to_owned()),
        enabled: Set(true),
        alias: Set(None),
        metadata: Set(serde_json::json!({})),
        checksum: Set(vec![]),
        ..Default::default()
    })
    .await?;

// INSERT bulk devuelve InsertResult de SeaORM. UPDATE/DELETE bulk
// devuelven rows_affected y deben ejecutarse con un filtro deliberado.
let _inserted = accounts
    .insert_many(vec![
        entities::account::ActiveModel {
            email: Set("bulk-one@example.test".to_owned()),
            enabled: Set(true),
            alias: Set(None),
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        },
        entities::account::ActiveModel {
            email: Set("bulk-two@example.test".to_owned()),
            enabled: Set(true),
            alias: Set(None),
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        },
    ])
    .await?;

let removed_one = accounts.delete_by_id(updated.id).await?;
assert_eq!(removed_one.rows_affected, 1);

let removed_bulk = accounts
    .delete_many()
    .filter(entities::account::Column::Email.starts_with("bulk-"))
    .exec(database.orm())
    .await?;
assert_eq!(removed_bulk.rows_affected, 2);

~~~

No interpretes `InsertResult` como un contador portable de filas afectadas: los metadatos de inserción dependen del motor. `update_many` y `delete_many` informan `rows_affected`. Los métodos `update`/`delete` rechazan un ActiveModel sin todos los campos de clave primaria, en lugar de ejecutar una modificación ambigua.

La API expone las capacidades por motor para no asumir que RETURNING tiene la misma disponibilidad:

~~~rust
let capabilities = database.capabilities();
assert!(capabilities.upsert);

// INSERT RETURNING solo está anunciado para SQLite y PostgreSQL.
// MySQL/MariaDB usan el camino exec + lectura posterior.
if capabilities.insert_returning {
    // Usa exec_with_returning en el Insert builder nativo.
}
~~~

Para upsert, usa el builder tipado de SeaORM. El mismo ejemplo se prueba contra los cuatro motores:

~~~rust
use rustflow_server::sea_orm::sea_query::OnConflict;

accounts
    .insert_builder(new_account)
    .on_conflict(
        OnConflict::column(entities::account::Column::Email)
            .update_columns([entities::account::Column::Enabled])
            .to_owned(),
    )
    .exec(database.orm())
    .await?;
~~~

SQLite/PostgreSQL generan ON CONFLICT. MySQL/MariaDB generan ON DUPLICATE KEY UPDATE; una colisión en cualquier clave única aplicable puede activar el update, por lo que no debes asumir el mismo targeting exacto entre motores.

Para relaciones, declara el grafo en las entidades SeaORM y usa loaders explícitos. Por ejemplo, `load_many` carga las notas para un conjunto de cuentas mediante una consulta por salto; `load_many_to_many` requiere la entidad de unión. Leer un modelo no dispara consultas ocultas. La suite ampliada verifica dos saltos anidados con exactamente una consulta batch por salto, comprueba que un destino `belongs_to` ausente se representa como `None`, y cubre attach, update, detach y borrado en cascada de filas de unión. Las pruebas de integración aplican el ciclo de mutación a los cuatro motores.

~~~rust
use rustflow_server::orm::load_many;

let parents = accounts.all(accounts.select()).await?;
let notes_by_account =
    load_many(&parents, entities::note::Entity, database.orm()).await?;
~~~


### Filtrar y ordenar relaciones por separado

El loader también recibe una consulta Select tipada. Puedes filtrar y ordenar los registros relacionados sin cambiar la consulta que obtuvo los padres:

~~~rust
use rustflow_server::orm::{load_many, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, QueryFilter, QueryOrder};

let posts_by_user = load_many(
    &users,
    RelationLoadOptions::<entities::post::Entity>::new().modify(|query| {
        query
            .filter(entities::post::Column::Published.eq(true))
            .order_by_desc(entities::post::Column::CreatedAt)
            .order_by_asc(entities::post::Column::Id)
    }),
    database.orm(),
)
.await?;
~~~

El filtro solo afecta a la consulta relacionada. Añade una segunda columna estable, normalmente la clave primaria, para obtener un orden determinista. Un limit aplicado al Select limita el batch completo; no es un top-N independiente por cada padre.

### Proyecciones sin cargar todas las columnas

No decodifiques una selección parcial como si fuera el Model completo. Define un DTO con FromQueryResult y conserva la clave foránea que permite asociarlo al padre:

~~~rust
use rustflow_server::orm::{load_many_projected, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, FromQueryResult, QueryOrder};

#[derive(Clone, Debug, FromQueryResult)]
struct PostSummary {
    id: i64,
    user_id: i64,
    title: String,
}

let summaries = load_many_projected(
    &users,
    entities::post::Column::UserId,
    RelationLoadOptions::<entities::post::Entity>::new().modify(|query| {
        query
            .select_only()
            .column(entities::post::Column::Id)
            .column(entities::post::Column::UserId)
            .column(entities::post::Column::Title)
            .order_by_desc(entities::post::Column::Id)
    }),
    |user| user.id,
    |post: &PostSummary| post.user_id,
    database.orm(),
)
.await?;
~~~

El helper agrega un IN parametrizado con las claves de todos los padres, ejecuta una consulta batch y reconstruye los grupos en el orden original de los padres. Los dos extractores deben identificar la misma clave de asociación. Este helper cubre proyecciones has-many y devuelve DTOs, no entidades completas.

### Grafos explícitos de relaciones anidadas

RelationGraph permite declarar varias ramas y varios niveles. Cada relación tiene sus propias opciones de filtro y orden:

~~~rust
use rustflow_server::orm::{RelationGraph, RelationLoadOptions};
use rustflow_server::sea_orm::{ColumnTrait, QueryOrder};

let post_graph = RelationGraph::<entities::post::Model, _>::new()
    .with_many(
        "comments",
        RelationLoadOptions::<entities::comment::Entity>::new()
            .modify(|query| query.order_by_asc(entities::comment::Column::CreatedAt)),
        RelationGraph::new(),
    )?;

let graph = RelationGraph::<entities::user::Model, _>::new()
    .with_many(
        "posts",
        RelationLoadOptions::<entities::post::Entity>::new()
            .modify(|query| query.order_by_desc(entities::post::Column::CreatedAt)),
        post_graph,
    )?
    .with_one(
        "profile",
        RelationLoadOptions::<entities::profile::Entity>::new(),
        RelationGraph::new(),
    )?
    .with_many_to_many(
        "roles",
        RelationLoadOptions::<entities::role::Entity>::new(),
        entities::user_role::Entity,
        RelationGraph::new(),
    )?
    .with_max_depth(4)?;

let loaded = graph.load(&users, database.orm()).await?;
let posts = loaded.many::<entities::post::Model>("posts").unwrap();
let comments = loaded.many::<entities::comment::Model>("posts.comments").unwrap();
let profiles = loaded.one::<entities::profile::Model>("profile").unwrap();
~~~

El límite predeterminado es de 8 niveles y el máximo configurable es de 16. Los nombres de ramas hermanas deben ser únicos. Recupera colecciones con many(path) y relaciones uno-a-uno con one(path). En una ruta anidada, cada grupo corresponde al orden aplanado de los modelos de la rama anterior. El loader many-to-many utiliza la entidad junction explícita. El número de consultas depende de las aristas y de su tipo, no del número de padres. Las ramas sin padres intermedios no ejecutan consultas posteriores. No hay carga perezosa implícita.

También puedes añadir una proyección DTO mediante with_many_projected y recuperarla con many; ese DTO no es una entidad SeaORM y no admite ramas hijas.

### Modificar relaciones uno-a-uno y uno-a-muchos

attach_one_to_many mueve un modelo relacionado existente a otro padre mediante su columna foránea tipada:

~~~rust
use rustflow_server::orm::attach_one_to_many;

let transaction = database.begin_transaction().await?;
let post = attach_one_to_many(
    existing_post,
    entities::post::Column::UserId,
    user.id.into(),
    transaction.connection(),
)
.await?;
transaction.commit().await?;
~~~

detach_one_to_many establece el FK nullable en NULL. Si la columna es NOT NULL o se viola otra restricción, devuelve el error de base de datos; no elimina el registro.

replace_one_to_one separa la fila previa y asocia un reemplazo dentro de una transacción explícita:

~~~rust
use rustflow_server::orm::replace_one_to_one;

let transaction = database.begin_transaction().await?;
let profile = replace_one_to_one(
    Some(previous_profile),
    replacement_profile,
    entities::profile::Column::UserId,
    user.id.into(),
    &transaction,
)
.await?;
transaction.commit().await?;
~~~

Para una relación uno-a-uno real, el FK debe ser nullable y tener una restricción UNIQUE. Estos helpers no hacen commit ni rollback por ti: revierte la transacción si devuelven un error y confirma solo cuando todas las operaciones han salido bien. No se realizan cascadas destructivas implícitas.

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

La subfase 6.6.2 implementó el adaptador MongoDB de bajo nivel y no incluía un ODM, derives de modelos, relaciones SQL, un query builder portátil ni migraciones de documentos. La subfase 6.7.3 añadió después el ODM documental independiente, ya fusionado en `main` mediante PR #60. La matriz completa de motores se completó en la subfase 6.6.3; para el contrato ODM y sus límites, consulta la [guía ODM MongoDB](/guides/mongodb-odm/) y el [ADR 0021](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0021-mongodb-odm-architecture.md).

## 5. Errores de base de datos

DatabaseError::kind() devuelve una categoría estable sin exponer mensajes del driver, SQL, valores, nombres de restricciones ni URLs.

| DatabaseErrorKind | Condición | HTTP |
|---|---|---:|
| InvalidModelState | Modelo sin clave primaria válida o estado de escritura inválido | 400 |
| NotFound | Registro requerido ausente | 404 |
| UniqueViolation | Clave primaria o valor único duplicado | 409 |
| ForeignKeyViolation | Referencia ausente o eliminación bloqueada por relaciones | 409 |
| CheckViolation | El valor viola un CHECK de la base de datos | 422 |
| Unavailable | Pool cerrado o timeout de conexión | 503 |
| CommitOutcomeUnknown | Se perdió la conexión mientras se enviaba COMMIT | 503 |
| Query / Transaction / Migration | Fallo no clasificado de base de datos | 500 |

Los códigos se interpretan según el protocolo: PostgreSQL usa SQLSTATE 23505/23503/23514; SQLite usa 1555/2067/787/275; MySQL y MariaDB se clasifican por sus números de error para unicidad, FK y CHECK. Los mensajes HTTP son genéricos; conserva DatabaseError y su source para diagnóstico interno seguro.

**COMMIT ambiguo no significa rollback.** Cuando falla el transporte durante COMMIT, no se puede inferir si el servidor confirmó la transacción. La API devuelve CommitOutcomeUnknown. Comprueba primero un identificador estable o una clave de idempotencia; no repitas a ciegas una operación no idempotente.

## 6. Pool sizing y operación

El límite de conexiones se aplica por proceso, no por aplicación completa. Estima el uso total como máximo del pool × número de réplicas y reserva conexiones para migraciones, trabajadores y herramientas operativas. Configura límites compatibles con max_connections del servidor y mide saturación antes de incrementarlos; más conexiones no siempre significan mayor rendimiento.

Las migraciones de producción deben ejecutarse como un paso coordinado, antes del despliegue de nuevas versiones. No habilites migraciones destructivas automáticas en el arranque de cada réplica.

## Límites actuales

Phase 6.7.2 integra SeaORM 1.1.20 sobre el mismo pool SQLx tipado. Las pruebas de límites numéricos signed/unsigned por dialecto, conteos de consultas en carga anidada, mutaciones de relaciones y terminación real del backend PostgreSQL mientras `COMMIT` está activo pasaron en el SHA final de PR #59, que fue fusionada en `main` el 2026-10-31.

Limitaciones que deben respetarse:
- PostgreSQL no ofrece mapeo unsigned para `u8/u16/u32/u64` en SeaORM 1.1.20. `u8/u16/u32` se prueban en SQLite y MySQL/MariaDB; `u64` no tiene codec directo para SQLite y solo se prueba en MySQL/MariaDB mediante `BIGINT UNSIGNED`.
- El codec Decimal/BigDecimal de SeaORM 1.1.20 para SQLite pasa por f64. No promete precisión decimal arbitraria; el fixture usa 12.5 porque es exactamente representable. Usa enteros escalados o un codec textual canónico cuando el dominio requiera precisión exacta.
- MySQL y MariaDB comparten el backend/protocolo SQLx MySQL; las pruebas validan por separado la identidad y versión del servidor.
- Upsert y RETURNING siguen siendo operaciones dependientes del motor; no se promete paridad cuando MySQL/MariaDB y PostgreSQL/SQLite difieren.
- MongoDB conserva su adaptador documental independiente. Su ODM pertenece a Phase 6.7.3. Sesiones y autorización también tienen contratos separados; InMemorySessionStore continúa siendo volátil y local al proceso.

## Roadmap de persistencia

- **Phase 6.7.1:** cerrada y fusionada en PR #58; selecciona SeaORM 1.1.20 y define el contrato Data Mapper/ModelDiagnostics.
- **Phase 6.7.2:** integrada en `main` mediante PR #59; la implementación incluye CRUD/bulk, consultas tipadas, transacciones mixtas, codecs y loaders explícitos, con contrato en los cuatro motores SQL.
- **Phase 6.7.3:** ODM MongoDB tipado independiente, fusionado en `main` mediante PR #60 (HEAD final `659f1750d611e57315263216167d43375da23574`; merge `7ffb89b5ca36017a428cf3f828b68f590f8d404b`). La implementación y sus límites están en la [guía ODM MongoDB](/guides/mongodb-odm/) y [ADR 0021](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0021-mongodb-odm-architecture.md).
- **Phase 6.7.4:** aceptada y fusionada mediante PR #62 (HEAD final `28218456db8ac323731054e291023a0765474125`; merge `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`). Añade `RelationLoadOptions` para filtros/orden por relación, `load_many_projected` para DTOs has-many, `RelationGraph` con ramas/niveles explícitos (profundidad predeterminada 8, máxima 16) y helpers `attach_one_to_many`, `detach_one_to_many` y `replace_one_to_one`. Los límites (offset/limit global al batch, proyecciones como hojas, sin lazy loading ni cascadas implícitas) están documentados en [ADR 0022](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0022-advanced-sql-relation-loading.md).
- **Phase 6.7.5:** pendiente; aplicación CRUD generada que pruebe persistencia, relaciones, validación, transacciones y salida renderizada.
- **Phase 6.7.6:** pendiente; gate global de calidad, compatibilidad y madurez ORM/ODM.

La Phase 6.6 ya está completa y su matriz verificada está en `docs/DATABASE_SUPPORT_MATRIX.md`. El adaptador MongoDB de la 6.6.2 y el ODM independiente de Phase 6.7.3 están disponibles en `main`; el último se fusionó mediante PR #60 (`7ffb89b5ca36017a428cf3f828b68f590f8d404b`).

La auditoría de dependencias usa cargo audit en CI. Se mantiene una única excepción documentada para RUSTSEC-2023-0071 (rsa 0.9.10 a través de sqlx-mysql 0.8.6); el advisory no tiene una versión corregida y no se considera resuelto. Consulta ADR 0020 para el análisis de alcance y la condición para retirar esa excepción.

Para el alcance completo, consulta docs/DATABASES.md, docs/ORM_MODEL_CONTRACT.md, la referencia de disponibilidad de API (/reference/api-status/) y ROADMAP.md.
