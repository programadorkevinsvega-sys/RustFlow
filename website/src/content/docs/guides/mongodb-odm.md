---
title: ODM tipado para MongoDB
description: Modelos BSON tipados, repositorios explícitos, filtros, actualizaciones, proyecciones, agregaciones y sesiones MongoDB en RustFlow.
---

El ODM de RustFlow para MongoDB es una API documental experimental, independiente del SQL ORM. Utiliza el cliente oficial que ya administra `MongoDatabase`: no crea otro pool, no comparte entidades SeaORM y no traduce operaciones MongoDB a SQL.

## 1. Configurar MongoDB

Configura `RUSTFLOW_MONGODB_URI` y `RUSTFLOW_MONGODB_DATABASE` en el entorno. La URI admitida en este contrato es `mongodb://`; el resolver DNS SRV está deshabilitado. La configuración aplica timeouts de conexión, selección de servidor y operaciones.

```rust
use rustflow_server::{MongoConfig, MongoDatabase};

let config = MongoConfig::new(
    std::env::var("RUSTFLOW_MONGODB_URI").expect("set RUSTFLOW_MONGODB_URI"),
    std::env::var("RUSTFLOW_MONGODB_DATABASE").expect("set RUSTFLOW_MONGODB_DATABASE"),
)?;
let database = MongoDatabase::connect(&config).await?;
database.ping().await?;
```

## 2. Declarar un documento

Implementa `DocumentModel` para asociar el tipo Rust a una colección y al tipo de identificador. La propiedad del ID debe serializarse como BSON `_id`. Para que MongoDB genere un `ObjectId`, omite el campo cuando todavía es `None`.

```rust
use rustflow_server::mongodb_driver::bson::oid::ObjectId;
use rustflow_server::DocumentModel;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    email: String,
    age: i32,
    #[serde(default)]
    tags: Vec<String>,
}

impl DocumentModel for User {
    type Id = ObjectId;
    const COLLECTION: &'static str = "users";

    fn id(&self) -> Option<Self::Id> { self.id.clone() }
    fn set_id(&mut self, id: Self::Id) { self.id = Some(id); }
}
```

El trait no añade conexión ni carga implícita al modelo. Los documentos embebidos son structs/arrays Serde normales. Una referencia es un identificador explícito (por ejemplo, otro `ObjectId`) y solo se resuelve mediante otra consulta o un pipeline explícito como `$lookup`.

## 3. Insertar y consultar

Obtén el repositorio mediante `database.repository::<User>()`. Una inserción devuelve el modelo con el ID generado; los IDs suministrados por la aplicación se conservan.

```rust
use rustflow_server::{DocumentFilter, DocumentQuery};

let users = database.repository::<User>();
let saved = users.insert(User {
    id: None,
    email: "ada@example.test".to_owned(),
    age: 37,
    tags: vec!["rust".to_owned()],
}).await?;

let by_id = users.find_by_id(saved.id.clone().expect("insert generated an ID")).await?;
let page = users.find_many(
    DocumentQuery::new(
        DocumentFilter::new()
            .gte("age", 18)?
            .lt("age", 65)?,
    )
    .sort_desc("age")?
    .skip(0)
    .limit(50)?,
).await?;
```

El filtro también dispone de igualdad/desigualdad, `AND`/`OR`, existencia, null, `IN`/`NOT IN`, operadores de arrays y campos anidados. Para asociar un campo con su tipo Rust, declara un descriptor `DocumentField<T>` y utiliza los helpers `eq_field`, `gt_field`, `gte_field`, `lt_field` y sus equivalentes de actualización.

```rust
use rustflow_server::{DocumentField, DocumentUpdate};

const EMAIL: DocumentField<String> = DocumentField::new("email");
const AGE: DocumentField<i32> = DocumentField::new("age");

let filter = DocumentFilter::new()
    .eq_field(EMAIL, "ada@example.test".to_owned())?
    .gte_field(AGE, 18)?;

users.update_one(
    filter,
    DocumentUpdate::new()
        .inc_field(AGE, 1)?
        .set("email_verified", true)?,
    false,
).await?;
```

El descriptor liga la ruta que declaras con un tipo de valor; no inspecciona automáticamente los nombres Serde del struct. Mantén los descriptores junto al modelo y prueba sus rutas con BSON real.

## 4. Actualizar, reemplazar y eliminar

`DocumentUpdate` crea operadores MongoDB `$set`, `$unset`, `$inc`, `$push` y `$pull`. No acepta dos modificaciones sobre la misma ruta ni sobre rutas padre/hija que se solapen. El flag `upsert` de `update_one` y `update_many` se transmite al driver.

```rust
let outcome = users.update_one(
    DocumentFilter::new().eq("email", "ada@example.test")?,
    DocumentUpdate::new()
        .set("email_verified", true)?
        .push("tags", "mongodb")?,
    false,
).await?;

let removed = users.delete_one(
    DocumentFilter::new().eq("email", "ada@example.test")?,
).await?;
assert_eq!(removed.deleted_count, 1);
```

El repositorio incluye `insert_many`, `replace_one`, `update_many` y `delete_many`. Los bulk writes conservan los errores del driver. Una escritura masiva puede aplicar algunos documentos antes de fallar: ni las opciones ordered ni unordered convierten un bulk en transacción.

## 5. Proyecciones y agregaciones

Usa `find_many_as::<Dto>` para convertir una proyección a un DTO cuyos campos requeridos coincidan con los que devuelve MongoDB. No intentes deserializar una proyección incompleta en el modelo completo sin cubrir todos sus campos obligatorios.

```rust
#[derive(serde::Deserialize)]
struct UserSummary {
    #[serde(rename = "_id")]
    id: ObjectId,
    email: String,
}

let summaries = users.find_many_as::<UserSummary>(
    DocumentQuery::new(DocumentFilter::new())
        .include("email")?
        .sort_asc("email")?
        .limit(100)?,
).await?;
```

Las agregaciones usan etapas BSON nativas, sin traducir un query builder SQL:

```rust
use rustflow_server::mongodb_driver::bson::doc;

#[derive(serde::Deserialize)]
struct AgeCount {
    #[serde(rename = "_id")]
    age: i32,
    count: i32,
}

let counts = users.aggregate_as::<AgeCount>(vec![
    doc! { "$group": { "_id": "$age", "count": { "$sum": 1 } } },
    doc! { "$sort": { "_id": 1 } },
]).await?;
```

## 6. Índices y ciclo de vida

`DocumentBootstrap` crea una colección inexistente y los índices declarados; las declaraciones pueden incluir índices simples o compuestos y la opción de unicidad. El bootstrap verifica nombre, claves y unicidad, y no elimina/reemplaza índices automáticamente. TTL, sparse, partial, collation y otros parámetros no están todavía dentro del contrato de `DocumentIndex`. No existe un sistema de migraciones de documentos versionadas.

## 7. Sesiones y transacciones

Las sesiones usan el mismo cliente del adaptador. Una transacción exige un replica set o un clúster sharded; un MongoDB standalone no sirve. Pasa la sesión explícitamente a cada operación que deba participar y no ejecutes operaciones paralelas sobre una misma sesión transaccional.

```rust
let mut session = database.start_session().await?;
session.start_transaction().await?;

let inserted = users.insert_with_session(
    User {
        id: None,
        email: "transaction@example.test".to_owned(),
        age: 20,
        tags: vec![],
    },
    &mut session,
).await?;

session.commit_transaction().await?;
```

Si una operación falla dentro de la transacción, el código de la aplicación debe decidir si abortar; el driver también aborta una transacción todavía abierta al descartar la sesión. El repositorio no envuelve callbacks en reintentos automáticos. Un error con la etiqueta `UnknownTransactionCommitResult` representa un resultado de commit incierto: no presupongas rollback ni vuelvas a ejecutar una operación no idempotente sin reconciliar su resultado. Las operaciones con una sesión prestada no se cancelan mediante el timeout genérico del repositorio.

## 8. Campos ausentes, null y defaults

Con `Option<T>` de Serde, por defecto tanto un campo ausente como un BSON `null` suelen convertirse en `None`. `#[serde(default)]` aplica un valor predeterminado cuando el campo falta; no preserva la diferencia entre ausencia y null. Utiliza un tipo triestado/custom deserializer o BSON crudo cuando la aplicación tenga que distinguirlos.

## Límites actuales

La API es experimental y pre-1.0. El contrato no incluye carga perezosa, relaciones SQL, cascadas automáticas, migraciones SQL/documentales, un identity map global ni un segundo cliente MongoDB. Las operaciones nativas del driver quedan disponibles a través de `MongoDatabase::collection` para los casos que el facade tipado todavía no cubre. La subfase 6.7.3 no cierra la fase 6.7 global.

Consulta también la [guía general de base de datos](/guides/database/), el [estado de APIs](/reference/api-status/) y el [roadmap técnico en GitHub](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md).
