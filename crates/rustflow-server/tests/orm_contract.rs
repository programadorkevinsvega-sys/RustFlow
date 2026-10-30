use std::env;

use rustflow_server::{
    DatabaseBackend, DatabaseConfig, DatabasePool,
    orm::{load_many, load_one},
    sea_orm::{
        ActiveValue::{NotSet, Set},
        ColumnTrait, Condition, EntityTrait, ExprTrait, JoinType, QueryFilter, QueryOrder,
        QuerySelect, QueryTrait, RelationTrait, sea_query::Expr,
    },
};

mod entities {
    pub mod account {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_accounts")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub email: String,
            pub enabled: bool,
            pub alias: Option<String>,
            pub metadata: Json,
            pub checksum: Vec<u8>,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {
            #[sea_orm(has_many = "super::note::Entity")]
            Notes,
        }

        impl Related<super::note::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Notes.def()
            }
        }

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod note {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_notes")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub account_id: i64,
            pub title: String,
            pub body: Option<String>,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {
            #[sea_orm(
                belongs_to = "super::account::Entity",
                from = "Column::AccountId",
                to = "super::account::Column::Id"
            )]
            Account,
        }

        impl Related<super::account::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Account.def()
            }
        }

        impl ActiveModelBehavior for ActiveModel {}
    }
}

fn integration_url(variable: &str) -> Option<String> {
    match env::var(variable) {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("{variable} must be configured in CI");
        }
        _ => {
            eprintln!("Skipping {variable} contract outside CI: variable is unset");
            None
        }
    }
}

async fn create_schema(database: &DatabasePool) {
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_notes", [])
        .await
        .expect("drop notes table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_accounts", [])
        .await
        .expect("drop accounts table");

    let (account_id, enabled, alias, metadata, checksum, note_id, body) =
        match database.backend() {
            DatabaseBackend::Sqlite => (
                "INTEGER PRIMARY KEY AUTOINCREMENT",
                "BOOLEAN NOT NULL",
                "TEXT NULL",
                "JSON NOT NULL",
                "BLOB NOT NULL",
                "INTEGER PRIMARY KEY AUTOINCREMENT",
                "TEXT NULL",
            ),
            DatabaseBackend::Postgres => (
                "BIGSERIAL PRIMARY KEY",
                "BOOLEAN NOT NULL",
                "VARCHAR(255) NULL",
                "JSONB NOT NULL",
                "BYTEA NOT NULL",
                "BIGSERIAL PRIMARY KEY",
                "TEXT NULL",
            ),
            DatabaseBackend::MySql => (
                "BIGINT AUTO_INCREMENT PRIMARY KEY",
                "BOOLEAN NOT NULL",
                "VARCHAR(255) NULL",
                "JSON NOT NULL",
                "LONGBLOB NOT NULL",
                "BIGINT AUTO_INCREMENT PRIMARY KEY",
                "TEXT NULL",
            ),
        };

    let accounts = format!(
        "CREATE TABLE rustflow_orm_accounts (
            id {account_id},
            email VARCHAR(255) NOT NULL UNIQUE,
            enabled {enabled},
            alias {alias},
            metadata {metadata},
            checksum {checksum}
        )"
    );
    let notes = format!(
        "CREATE TABLE rustflow_orm_notes (
            id {note_id},
            account_id BIGINT NOT NULL,
            title VARCHAR(255) NOT NULL,
            body {body},
            FOREIGN KEY (account_id) REFERENCES rustflow_orm_accounts(id) ON DELETE CASCADE
        )"
    );
    database
        .execute_raw(&accounts, [])
        .await
        .expect("create accounts table");
    database
        .execute_raw(&notes, [])
        .await
        .expect("create notes table");
}

async fn exercise_orm(database: &DatabasePool) {
    create_schema(database).await;

    let accounts = database.entity::<entities::account::Entity>();
    let notes = database.entity::<entities::note::Entity>();

    let first = accounts
        .insert(entities::account::ActiveModel {
            email: Set("ada@example.test".to_owned()),
            enabled: Set(true),
            alias: Set(Some("Ada".to_owned())),
            metadata: Set(serde_json::json!({"role": "writer", "level": 3})),
            checksum: Set(vec![0, 1, 2, 255]),
            ..Default::default()
        })
        .await
        .expect("insert first account");
    let second = accounts
        .insert(entities::account::ActiveModel {
            email: Set("grace@example.test".to_owned()),
            enabled: Set(false),
            alias: Set(None),
            metadata: Set(serde_json::json!({"role": "reader"})),
            checksum: Set(vec![9, 8, 7]),
            ..Default::default()
        })
        .await
        .expect("insert second account");

    assert!(first.id > 0);
    assert!(second.id > first.id);
    assert_eq!(first.metadata["level"], 3);
    assert_eq!(first.checksum, [0, 1, 2, 255]);
    assert_eq!(second.alias, None);

    let first_note = entities::note::ActiveModel {
        account_id: Set(first.id),
        title: Set("first note".to_owned()),
        body: Set(None),
        ..Default::default()
    };
    let second_note = entities::note::ActiveModel {
        account_id: Set(first.id),
        title: Set("second note".to_owned()),
        body: Set(Some("body".to_owned())),
        ..Default::default()
    };
    let third_note = entities::note::ActiveModel {
        account_id: Set(second.id),
        title: Set("third note".to_owned()),
        body: Set(None),
        ..Default::default()
    };
    let inserted_notes = notes
        .insert_many([first_note, second_note, third_note])
        .await
        .expect("bulk insert notes");
    assert!(inserted_notes.last_insert_id > 0);

    let fetched = accounts
        .find_by_id(first.id)
        .await
        .expect("find account")
        .expect("account exists");
    assert_eq!(fetched.email, "ada@example.test");

    let mut changed: entities::account::ActiveModel = fetched.clone().into();
    changed.email = Set("ada.lovelace@example.test".to_owned());
    let updated = accounts.update(changed).await.expect("update account");
    assert_eq!(updated.email, "ada.lovelace@example.test");

    let filtered = accounts
        .select()
        .filter(
            Condition::any()
                .add(entities::account::Column::Email.contains("example.test"))
                .add(
                    Condition::all()
                        .add(entities::account::Column::Enabled.eq(true))
                        .add(entities::account::Column::Alias.is_not_null()),
                ),
        )
        .order_by_asc(entities::account::Column::Email)
        .limit(10);
    let matching = accounts.all(filtered).await.expect("nested filter query");
    assert_eq!(matching.len(), 2);

    let joined = entities::account::Entity::find()
        .join(
            JoinType::LeftJoin,
            entities::account::Relation::Notes.def(),
        )
        .all(database.orm())
        .await
        .expect("left join query");
    assert_eq!(joined.len(), 2);

    let parents = accounts
        .all(
            accounts
                .select()
                .order_by_asc(entities::account::Column::Id),
        )
        .await
        .expect("load parent rows");
    let children = load_many(&parents, entities::note::Entity, database.orm())
        .await
        .expect("batch load one-to-many relation");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].len(), 2);
    assert_eq!(children[1].len(), 1);

    let loaded_one = load_one(&parents, entities::account::Entity, database.orm())
        .await;
    assert!(loaded_one.is_err(), "a model is not related to itself by default");

    let grouped: Vec<(i64, i64)> = entities::note::Entity::find()
        .select_only()
        .column(entities::note::Column::AccountId)
        .column_as(entities::note::Column::Id.count(), "note_count")
        .group_by(entities::note::Column::AccountId)
        .having(entities::note::Column::Id.count().gt(0))
        .into_tuple()
        .all(database.orm())
        .await
        .expect("grouped aggregate query");
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].1 + grouped[1].1, 3);

    let subquery = entities::note::Entity::find()
        .select_only()
        .column(entities::note::Column::AccountId)
        .into_query();
    let accounts_with_notes = accounts
        .all(
            accounts
                .select()
                .filter(entities::account::Column::Id.in_subquery(subquery)),
        )
        .await
        .expect("subquery filter");
    assert_eq!(accounts_with_notes.len(), 2);

    let changed_rows = notes
        .update_many()
        .col_expr(
            entities::note::Column::Title,
            Expr::val("renamed note".to_owned()),
        )
        .filter(entities::note::Column::AccountId.eq(first.id))
        .exec(database.orm())
        .await
        .expect("bulk update");
    assert_eq!(changed_rows.rows_affected, 2);

    let after_bulk_update = notes
        .all(notes.select().filter(entities::note::Column::AccountId.eq(first.id)))
        .await
        .expect("read bulk-updated notes");
    assert!(after_bulk_update.iter().all(|note| note.title == "renamed note"));

    let delete_rows = notes
        .delete_many()
        .filter(entities::note::Column::AccountId.eq(second.id))
        .exec(database.orm())
        .await
        .expect("bulk delete");
    assert_eq!(delete_rows.rows_affected, 1);

    let transaction = database
        .begin_transaction()
        .await
        .expect("begin rollback transaction");
    transaction
        .entity::<entities::note::Entity>()
        .insert(entities::note::ActiveModel {
            account_id: Set(first.id),
            title: Set("rolled back".to_owned()),
            body: NotSet,
            ..Default::default()
        })
        .await
        .expect("insert inside transaction");
    transaction.rollback().await.expect("rollback transaction");

    let rolled_back = notes
        .one(
            notes
                .select()
                .filter(entities::note::Column::Title.eq("rolled back")),
        )
        .await
        .expect("query rollback result");
    assert!(rolled_back.is_none());

    let commit = database.begin_transaction().await.expect("begin commit transaction");
    commit
        .entity::<entities::note::Entity>()
        .insert(entities::note::ActiveModel {
            account_id: Set(first.id),
            title: Set("committed".to_owned()),
            body: NotSet,
            ..Default::default()
        })
        .await
        .expect("insert inside committed transaction");
    commit.commit().await.expect("commit transaction");

    let committed = notes
        .one(
            notes
                .select()
                .filter(entities::note::Column::Title.eq("committed")),
        )
        .await
        .expect("query commit result");
    assert!(committed.is_some());

    let duplicate = accounts
        .insert(entities::account::ActiveModel {
            email: Set("ada.lovelace@example.test".to_owned()),
            enabled: Set(true),
            alias: NotSet,
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        })
        .await
        .expect_err("unique email must be rejected");
    assert_eq!(duplicate.to_http_error().status_code(), http::StatusCode::CONFLICT);

    let removed = accounts
        .delete_by_id(second.id)
        .await
        .expect("delete account by id");
    assert_eq!(removed.rows_affected, 1);
}

async fn database_from_url(url: &str) -> DatabasePool {
    let config = DatabaseConfig::new(url)
        .expect("valid test database URL")
        .with_max_connections(4)
        .expect("valid pool size");
    DatabasePool::connect(&config)
        .await
        .expect("connect to ORM contract database")
}

#[tokio::test]
async fn sqlite_orm_contract() {
    let config = DatabaseConfig::new("sqlite::memory:")
        .expect("SQLite URL")
        .with_max_connections(1)
        .expect("single SQLite connection");
    let database = DatabasePool::connect(&config)
        .await
        .expect("connect SQLite ORM database");
    exercise_orm(&database).await;
}

#[tokio::test]
async fn postgres_orm_contract() {
    let Some(url) = integration_url("RUSTFLOW_TEST_DATABASE_URL") else {
        return;
    };
    let database = database_from_url(&url).await;
    exercise_orm(&database).await;
}

#[tokio::test]
async fn mysql_orm_contract() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MYSQL_URL") else {
        return;
    };
    let database = database_from_url(&url).await;
    exercise_orm(&database).await;
}

#[tokio::test]
async fn mariadb_orm_contract() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MARIADB_URL") else {
        return;
    };
    let database = database_from_url(&url).await;
    exercise_orm(&database).await;
}
