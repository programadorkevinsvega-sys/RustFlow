use std::{
    env,
    future::Future,
    hint::black_box,
    time::{Duration, Instant},
};

use rustflow_server::{
    DatabaseConfig, DatabasePool, DocumentFilter, DocumentModel, DocumentQuery, DocumentUpdate,
    MongoConfig, MongoDatabase,
    mongodb_driver::bson::oid::ObjectId,
    orm::load_many,
    sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect},
};
use serde::{Deserialize, Serialize};

mod accounts {
    use super::notes;
    use rustflow_server::sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "rustflow_bench_accounts")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub email: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::notes::Entity")]
        Notes,
    }

    impl Related<notes::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Notes.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

mod notes {
    use rustflow_server::sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "rustflow_bench_notes")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub account_id: i64,
        pub title: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::accounts::Entity",
            from = "Column::AccountId",
            to = "super::accounts::Column::Id"
        )]
        Account,
    }

    impl Related<super::accounts::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Account.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    value: i64,
    label: String,
}

impl DocumentModel for BenchDocument {
    type Id = ObjectId;
    const COLLECTION: &'static str = "rustflow_bench_documents";

    fn id(&self) -> Option<Self::Id> {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = Some(id);
    }
}

#[derive(Clone, Copy)]
enum SqlDialect {
    Sqlite,
    Postgres,
    MySql,
}

async fn measure<F, Fut>(name: &str, iterations: usize, mut operation: F)
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = ()>,
{
    for iteration in 0..iterations.min(5) {
        operation(iteration).await;
    }

    let started = Instant::now();
    for iteration in 0..iterations {
        operation(iteration).await;
    }
    let elapsed = started.elapsed();
    let nanos_per_operation = elapsed.as_nanos() as f64 / iterations as f64;
    println!(
        "benchmark={name} iterations={iterations} elapsed_ms={:.3} ns_per_operation={nanos_per_operation:.1}",
        elapsed.as_secs_f64() * 1_000.0
    );
}

fn iterations() -> usize {
    env::var("RUSTFLOW_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(100)
}

fn bulk_size() -> usize {
    env::var("RUSTFLOW_BENCH_BULK_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=1_000).contains(value))
        .unwrap_or(50)
}

async fn run_sql_benchmarks(
    label: &'static str,
    url: &str,
    dialect: SqlDialect,
    iteration_count: usize,
    batch_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let maximum = if matches!(dialect, SqlDialect::Sqlite) {
        1
    } else {
        4
    };
    let config = DatabaseConfig::new(url)?
        .with_max_connections(maximum)?
        .with_acquire_timeout(Duration::from_secs(10))?;
    let database = DatabasePool::connect(&config).await?;

    let (accounts_ddl, notes_ddl) = match dialect {
        SqlDialect::Sqlite => (
            "CREATE TABLE rustflow_bench_accounts (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE)",
            "CREATE TABLE rustflow_bench_notes (id INTEGER PRIMARY KEY AUTOINCREMENT, account_id INTEGER NOT NULL REFERENCES rustflow_bench_accounts(id), title TEXT NOT NULL)",
        ),
        SqlDialect::Postgres => (
            "CREATE TABLE rustflow_bench_accounts (id BIGSERIAL PRIMARY KEY, email TEXT NOT NULL UNIQUE)",
            "CREATE TABLE rustflow_bench_notes (id BIGSERIAL PRIMARY KEY, account_id BIGINT NOT NULL REFERENCES rustflow_bench_accounts(id), title TEXT NOT NULL)",
        ),
        SqlDialect::MySql => (
            "CREATE TABLE rustflow_bench_accounts (id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY, email VARCHAR(255) NOT NULL UNIQUE) ENGINE=InnoDB",
            "CREATE TABLE rustflow_bench_notes (id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY, account_id BIGINT NOT NULL, title VARCHAR(255) NOT NULL, INDEX rustflow_bench_notes_account_id (account_id), CONSTRAINT rustflow_bench_notes_account_fk FOREIGN KEY (account_id) REFERENCES rustflow_bench_accounts(id)) ENGINE=InnoDB",
        ),
    };

    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_bench_notes", [])
        .await?;
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_bench_accounts", [])
        .await?;
    database.execute_raw(accounts_ddl, []).await?;
    database.execute_raw(notes_ddl, []).await?;

    println!(
        "target={label} backend={:?} max_connections={maximum} iterations={iteration_count} bulk_size={batch_size}",
        database.backend()
    );

    let accounts = database.entity::<accounts::Entity>();
    let notes = database.entity::<notes::Entity>();

    measure(
        &format!("{label}_orm_crud_round_trip"),
        iteration_count,
        |iteration| {
            let repository = &accounts;
            let email = format!("crud-{label}-{iteration}@example.test");
            async move {
                let inserted = repository
                    .insert(accounts::ActiveModel {
                        email: Set(email),
                        ..Default::default()
                    })
                    .await
                    .expect("benchmark ORM insert");
                let fetched = repository
                    .find_by_id(inserted.id)
                    .await
                    .expect("benchmark ORM lookup")
                    .expect("benchmark row exists");
                let updated = repository
                    .update(accounts::ActiveModel {
                        id: Set(fetched.id),
                        email: Set(format!("updated-{label}-{iteration}@example.test")),
                    })
                    .await
                    .expect("benchmark ORM update");
                repository
                    .delete_by_id(updated.id)
                    .await
                    .expect("benchmark ORM delete");
                black_box(updated.id);
            }
        },
    )
    .await;

    measure(
        &format!("{label}_orm_bulk_insert_{batch_size}"),
        iteration_count,
        |iteration| {
            let repository = &accounts;
            let database = &database;
            async move {
                let rows = (0..batch_size).map(|offset| accounts::ActiveModel {
                    email: Set(format!("bulk-{label}-{iteration}-{offset}@example.test")),
                    ..Default::default()
                });
                repository
                    .insert_many(rows)
                    .await
                    .expect("benchmark ORM bulk insert");
                database
                    .execute_raw("DELETE FROM rustflow_bench_accounts", [])
                    .await
                    .expect("clean benchmark bulk rows");
            }
        },
    )
    .await;

    let mut parents = Vec::with_capacity(20);
    for parent_index in 0..20 {
        let parent = accounts
            .insert(accounts::ActiveModel {
                email: Set(format!("seed-{label}-{parent_index}@example.test")),
                ..Default::default()
            })
            .await?;
        for child_index in 0..4 {
            notes
                .insert(notes::ActiveModel {
                    account_id: Set(parent.id),
                    title: Set(format!("note-{parent_index}-{child_index}")),
                    ..Default::default()
                })
                .await?;
        }
        parents.push(parent);
    }

    measure(
        &format!("{label}_orm_query_builder_and_execute"),
        iteration_count,
        |_| {
            let repository = &accounts;
            async move {
                let result = repository
                    .all(
                        repository
                            .select()
                            .filter(accounts::Column::Email.contains("seed-"))
                            .order_by_asc(accounts::Column::Id)
                            .limit(20),
                    )
                    .await
                    .expect("benchmark ORM query");
                black_box(result.len());
            }
        },
    )
    .await;

    measure(
        &format!("{label}_orm_relation_load_many_20x4"),
        iteration_count,
        |_| {
            let parents = &parents;
            let database = &database;
            async move {
                let loaded = load_many(
                    parents,
                    notes::Entity::find().order_by_asc(notes::Column::Id),
                    database.orm(),
                )
                .await
                .expect("benchmark relation batch load");
                let row_count = loaded.iter().map(Vec::len).sum::<usize>();
                black_box(row_count);
            }
        },
    )
    .await;

    database
        .execute_raw("DROP TABLE rustflow_bench_notes", [])
        .await?;
    database
        .execute_raw("DROP TABLE rustflow_bench_accounts", [])
        .await?;
    database.close().await;
    Ok(())
}

async fn run_mongodb_benchmarks(
    uri: &str,
    iteration_count: usize,
    batch_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = MongoConfig::new(uri, "rustflow_phase676_bench")?
        .with_operation_timeout(Duration::from_secs(10))?;
    let database = MongoDatabase::connect(&config).await?;
    let _ = database.run_with_timeout(database.database().drop()).await;
    let documents = database.repository::<BenchDocument>();

    println!(
        "target=mongodb server=uri-configured iterations={iteration_count} bulk_size={batch_size}"
    );

    measure(
        "mongodb_odm_crud_round_trip",
        iteration_count,
        |iteration| {
            let repository = &documents;
            async move {
                let inserted = repository
                    .insert(BenchDocument {
                        id: None,
                        value: iteration as i64,
                        label: format!("crud-{iteration}"),
                    })
                    .await
                    .expect("benchmark ODM insert");
                let id = inserted.id.expect("ODM generated ObjectId");
                let fetched = repository
                    .find_by_id(id)
                    .await
                    .expect("benchmark ODM lookup")
                    .expect("benchmark ODM document exists");
                repository
                    .update_one(
                        DocumentFilter::new()
                            .eq("_id", id)
                            .expect("benchmark ODM ID filter"),
                        DocumentUpdate::new()
                            .set("value", iteration as i64 + 1)
                            .expect("benchmark ODM update"),
                        false,
                    )
                    .await
                    .expect("benchmark ODM update execution");
                repository
                    .delete_one(
                        DocumentFilter::new()
                            .eq("_id", id)
                            .expect("benchmark ODM delete filter"),
                    )
                    .await
                    .expect("benchmark ODM delete");
                black_box(fetched.value);
            }
        },
    )
    .await;

    measure(
        &format!("mongodb_odm_bulk_insert_{batch_size}"),
        iteration_count,
        |iteration| {
            let repository = &documents;
            async move {
                let batch = (0..batch_size)
                    .map(|offset| BenchDocument {
                        id: None,
                        value: offset as i64,
                        label: format!("bulk-{iteration}-{offset}"),
                    })
                    .collect::<Vec<_>>();
                repository
                    .insert_many(batch, true)
                    .await
                    .expect("benchmark ODM bulk insert");
                repository
                    .delete_many(DocumentFilter::new())
                    .await
                    .expect("clean benchmark ODM bulk rows");
            }
        },
    )
    .await;

    database
        .run_with_timeout(database.database().drop())
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iteration_count = iterations();
    let batch_size = bulk_size();

    println!(
        "RustFlow persistence benchmark: iterations={iteration_count} bulk_size={batch_size} architecture={}",
        std::env::consts::ARCH
    );
    println!(
        "For reproducibility, record rustc --version, Cargo.lock revision, CI service image tags, and runner hardware."
    );

    let mut sql_targets = vec![("sqlite", "sqlite::memory:".to_owned(), SqlDialect::Sqlite)];

    for (label, variable, dialect) in [
        (
            "postgres",
            "RUSTFLOW_TEST_DATABASE_URL",
            SqlDialect::Postgres,
        ),
        ("mysql", "RUSTFLOW_TEST_MYSQL_URL", SqlDialect::MySql),
        ("mariadb", "RUSTFLOW_TEST_MARIADB_URL", SqlDialect::MySql),
    ] {
        if let Ok(url) = env::var(variable) {
            if !url.trim().is_empty() {
                sql_targets.push((label, url, dialect));
            } else {
                println!("skip_target={label} reason=empty_{variable}");
            }
        } else {
            println!("skip_target={label} reason=missing_{variable}");
        }
    }

    for (label, url, dialect) in sql_targets {
        run_sql_benchmarks(label, &url, dialect, iteration_count, batch_size).await?;
    }

    match env::var("RUSTFLOW_TEST_MONGODB_URI") {
        Ok(uri) if !uri.trim().is_empty() => {
            run_mongodb_benchmarks(&uri, iteration_count, batch_size).await?;
        }
        _ => println!("skip_target=mongodb reason=missing_RUSTFLOW_TEST_MONGODB_URI"),
    }

    Ok(())
}
