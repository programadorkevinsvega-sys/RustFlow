use std::{env, str::FromStr};

use rustflow_server::{
    DatabaseBackend, DatabaseConfig, DatabasePool,
    orm::{load_many, load_many_to_many, load_one},
    sea_orm::{
        ActiveValue::{NotSet, Set},
        ColumnTrait, Condition, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
        QueryTrait, RelationTrait,
        sea_query::{Expr, ExprTrait},
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
            #[sea_orm(has_one = "super::profile::Entity")]
            Profile,
            #[sea_orm(has_many = "super::account_tag::Entity")]
            AccountTags,
        }

        impl Related<super::note::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Notes.def()
            }
        }

        impl Related<super::profile::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Profile.def()
            }
        }

        impl Related<super::tag::Entity> for Entity {
            fn to() -> RelationDef {
                super::account_tag::Relation::Tag.def()
            }

            fn via() -> Option<RelationDef> {
                Some(Relation::AccountTags.def())
            }
        }

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod profile {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_profiles")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub account_id: i64,
            pub bio: String,
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

    pub mod tag {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_tags")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub name: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {
            #[sea_orm(has_many = "super::account_tag::Entity")]
            AccountTags,
        }

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod account_tag {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_account_tags")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub account_id: i64,
            pub tag_id: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {
            #[sea_orm(
                belongs_to = "super::account::Entity",
                from = "Column::AccountId",
                to = "super::account::Column::Id"
            )]
            Account,
            #[sea_orm(
                belongs_to = "super::tag::Entity",
                from = "Column::TagId",
                to = "super::tag::Column::Id"
            )]
            Tag,
        }

        impl Related<super::account::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Account.def()
            }
        }

        impl Related<super::tag::Entity> for Entity {
            fn to() -> RelationDef {
                Relation::Tag.def()
            }
        }

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod codec {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
        #[sea_orm(
            rs_type = "String",
            db_type = "String(StringLen::N(12))",
            enum_name = "rustflow_orm_codec_state"
        )]
        pub enum State {
            #[sea_orm(string_value = "active")]
            Active,
            #[sea_orm(string_value = "paused")]
            Paused,
        }

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_codecs")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub signed_value: i64,
            pub unsigned_value: u64,
            pub ratio: f64,
            pub decimal_value: Decimal,
            pub big_decimal_value: BigDecimal,
            pub uuid_value: Uuid,
            pub event_date: Date,
            pub event_time: Time,
            pub timestamp_value: DateTime,
            pub state: State,
            pub optional_value: Option<String>,
            pub json_value: Json,
            pub bytes_value: Vec<u8>,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod mysql_unsigned {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_mysql_unsigned")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub value: u64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

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
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_mysql_unsigned", [])
        .await
        .expect("drop MySQL-family unsigned codec table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_codecs", [])
        .await
        .expect("drop codec table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_account_tags", [])
        .await
        .expect("drop account tags table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_tags", [])
        .await
        .expect("drop tags table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_notes", [])
        .await
        .expect("drop notes table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_profiles", [])
        .await
        .expect("drop profiles table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_accounts", [])
        .await
        .expect("drop accounts table");

    let (account_id, enabled, alias, metadata, checksum, note_id, body) = match database.backend() {
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
    let profiles = format!(
        "CREATE TABLE rustflow_orm_profiles (
            id {note_id},
            account_id BIGINT NOT NULL UNIQUE,
            bio VARCHAR(255) NOT NULL,
            FOREIGN KEY (account_id) REFERENCES rustflow_orm_accounts(id) ON DELETE CASCADE
        )"
    );
    database
        .execute_raw(&profiles, [])
        .await
        .expect("create profiles table");
    let tags = format!(
        "CREATE TABLE rustflow_orm_tags (
            id {note_id},
            name VARCHAR(100) NOT NULL UNIQUE
        )"
    );
    let account_tags = format!(
        "CREATE TABLE rustflow_orm_account_tags (
            id {note_id},
            account_id BIGINT NOT NULL,
            tag_id BIGINT NOT NULL,
            UNIQUE (account_id, tag_id),
            FOREIGN KEY (account_id) REFERENCES rustflow_orm_accounts(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES rustflow_orm_tags(id) ON DELETE CASCADE
        )"
    );
    database
        .execute_raw(&tags, [])
        .await
        .expect("create tags table");
    database
        .execute_raw(&account_tags, [])
        .await
        .expect("create account tags table");

    let (
        codec_id,
        signed_type,
        ratio_type,
        decimal_type,
        big_decimal_type,
        uuid_type,
        date_type,
        time_type,
        timestamp_type,
        state_type,
        json_type,
        bytes_type,
    ) = match database.backend() {
        DatabaseBackend::Sqlite => (
            "INTEGER PRIMARY KEY AUTOINCREMENT",
            "INTEGER",
            "REAL",
            "TEXT",
            "TEXT",
            "BLOB",
            "TEXT",
            "TEXT",
            "TEXT",
            "VARCHAR(12)",
            "JSON",
            "BLOB",
        ),
        DatabaseBackend::Postgres => (
            "BIGSERIAL PRIMARY KEY",
            "BIGINT",
            "DOUBLE PRECISION",
            "NUMERIC(20,4)",
            "NUMERIC(35,10)",
            "UUID",
            "DATE",
            "TIME(6)",
            "TIMESTAMP(6)",
            "VARCHAR(12)",
            "JSONB",
            "BYTEA",
        ),
        DatabaseBackend::MySql => (
            "BIGINT AUTO_INCREMENT PRIMARY KEY",
            "BIGINT",
            "DOUBLE",
            "DECIMAL(20,4)",
            "DECIMAL(35,10)",
            "BINARY(16)",
            "DATE",
            "TIME(6)",
            "DATETIME(6)",
            "VARCHAR(12)",
            "JSON",
            "LONGBLOB",
        ),
    };
    let codecs = format!(
        "CREATE TABLE rustflow_orm_codecs (
            id {codec_id},
            signed_value {signed_type} NOT NULL,
            ratio {ratio_type} NOT NULL,
            decimal_value {decimal_type} NOT NULL,
            big_decimal_value {big_decimal_type} NOT NULL,
            uuid_value {uuid_type} NOT NULL,
            event_date {date_type} NOT NULL,
            event_time {time_type} NOT NULL,
            timestamp_value {timestamp_type} NOT NULL,
            state {state_type} NOT NULL,
            optional_value TEXT NULL,
            json_value {json_type} NOT NULL,
            bytes_value {bytes_type} NOT NULL
        )"
    );
    database
        .execute_raw(&codecs, [])
        .await
        .expect("create codec table");

    if database.backend() == DatabaseBackend::MySql {
        database
            .execute_raw(
                "CREATE TABLE rustflow_orm_mysql_unsigned (
                    id BIGINT AUTO_INCREMENT PRIMARY KEY,
                    value BIGINT UNSIGNED NOT NULL
                )",
                [],
            )
            .await
            .expect("create MySQL-family unsigned codec table");
    }
}

async fn exercise_type_codecs(database: &DatabasePool) {
    use rustflow_server::sea_orm::entity::prelude::{
        BigDecimal, Date, DateTime, Decimal, Time, Uuid,
    };

    let date = Date::from_ymd_opt(2026, 10, 30).expect("valid fixture date");
    let time = Time::from_hms_micro_opt(12, 34, 56, 123_456).expect("valid fixture time");
    let timestamp = DateTime::new(date, time);
    let uuid = Uuid::from_u128(0x1234_5678_1234_5678_1234_5678_1234_5678);
    let decimal = Decimal::new(12_345, 2);
    let big_decimal = BigDecimal::from_str("12345678901234567890.1234567890")
        .expect("valid arbitrary precision decimal");
    let json = serde_json::json!({"engine": "sql", "nullable": null});
    let bytes = vec![0, 1, 127, 128, 255];

    let codecs = database.entity::<entities::codec::Entity>();
    let inserted = codecs
        .insert(entities::codec::ActiveModel {
            signed_value: Set(-42),
            ratio: Set(0.125),
            decimal_value: Set(decimal),
            big_decimal_value: Set(big_decimal.clone()),
            uuid_value: Set(uuid),
            event_date: Set(date),
            event_time: Set(time),
            timestamp_value: Set(timestamp),
            state: Set(entities::codec::State::Active),
            optional_value: Set(None),
            json_value: Set(json.clone()),
            bytes_value: Set(bytes.clone()),
            ..Default::default()
        })
        .await
        .expect("insert all supported SQL codec values");
    let decoded = codecs
        .find_by_id(inserted.id)
        .await
        .expect("query inserted codec row")
        .expect("codec row exists");

    assert_eq!(decoded.signed_value, -42);
    assert!((decoded.ratio - 0.125).abs() < f64::EPSILON);
    assert_eq!(decoded.decimal_value, decimal);
    assert_eq!(decoded.big_decimal_value, big_decimal);
    assert_eq!(decoded.uuid_value, uuid);
    assert_eq!(decoded.event_date, date);
    assert_eq!(decoded.event_time, time);
    assert_eq!(decoded.timestamp_value, timestamp);
    assert_eq!(decoded.state, entities::codec::State::Active);
    assert_eq!(decoded.optional_value, None);
    assert_eq!(decoded.json_value, json);
    assert_eq!(decoded.bytes_value, bytes);

    // SQLx exposes unsigned 64-bit values only through the MySQL protocol.
    // Keep this engine-specific rather than implying SQLite/PostgreSQL support.
    if database.backend() == DatabaseBackend::MySql {
        let unsigned = database.entity::<entities::mysql_unsigned::Entity>();
        let inserted = unsigned
            .insert(entities::mysql_unsigned::ActiveModel {
                value: Set(u64::MAX),
                ..Default::default()
            })
            .await
            .expect("insert full-range MySQL-family unsigned value");
        let decoded = unsigned
            .find_by_id(inserted.id)
            .await
            .expect("query MySQL-family unsigned value")
            .expect("unsigned codec row exists");
        assert_eq!(decoded.value, u64::MAX);
    }
}

async fn exercise_orm(database: &DatabasePool) {
    create_schema(database).await;
    exercise_type_codecs(database).await;

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

    let first_tag = database
        .entity::<entities::tag::Entity>()
        .insert(entities::tag::ActiveModel {
            name: Set("language".to_owned()),
            ..Default::default()
        })
        .await
        .expect("insert first tag");
    let second_tag = database
        .entity::<entities::tag::Entity>()
        .insert(entities::tag::ActiveModel {
            name: Set("history".to_owned()),
            ..Default::default()
        })
        .await
        .expect("insert second tag");
    let account_tags = database.entity::<entities::account_tag::Entity>();
    account_tags
        .insert_many([
            entities::account_tag::ActiveModel {
                account_id: Set(first.id),
                tag_id: Set(first_tag.id),
                ..Default::default()
            },
            entities::account_tag::ActiveModel {
                account_id: Set(first.id),
                tag_id: Set(second_tag.id),
                ..Default::default()
            },
            entities::account_tag::ActiveModel {
                account_id: Set(second.id),
                tag_id: Set(second_tag.id),
                ..Default::default()
            },
        ])
        .await
        .expect("bulk insert many-to-many junction rows");

    database
        .entity::<entities::profile::Entity>()
        .insert(entities::profile::ActiveModel {
            account_id: Set(first.id),
            bio: Set("Analytical engine notes".to_owned()),
            ..Default::default()
        })
        .await
        .expect("insert one-to-one profile");

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
        .join(JoinType::LeftJoin, entities::account::Relation::Notes.def())
        .distinct()
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

    let profiles = load_one(&parents, entities::profile::Entity, database.orm())
        .await
        .expect("batch load one-to-one relation");
    assert_eq!(profiles.len(), 2);
    assert_eq!(
        profiles[0].as_ref().map(|profile| profile.bio.as_str()),
        Some("Analytical engine notes")
    );
    assert!(profiles[1].is_none());

    let loaded_tags = load_many_to_many(
        &parents,
        entities::tag::Entity,
        entities::account_tag::Entity,
        database.orm(),
    )
    .await
    .expect("batch load many-to-many tags");
    assert_eq!(loaded_tags.len(), 2);
    assert_eq!(loaded_tags[0].len(), 2);
    assert_eq!(loaded_tags[1].len(), 1);
    assert_eq!(loaded_tags[0][0].id, first_tag.id);
    assert_eq!(loaded_tags[0][1].id, second_tag.id);

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
            Expr::val("renamed note".to_owned()).into(),
        )
        .filter(entities::note::Column::AccountId.eq(first.id))
        .exec(database.orm())
        .await
        .expect("bulk update");
    assert_eq!(changed_rows.rows_affected, 2);

    let after_bulk_update = notes
        .all(
            notes
                .select()
                .filter(entities::note::Column::AccountId.eq(first.id)),
        )
        .await
        .expect("read bulk-updated notes");
    assert!(
        after_bulk_update
            .iter()
            .all(|note| note.title == "renamed note")
    );

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

    let commit = database
        .begin_transaction()
        .await
        .expect("begin commit transaction");
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
    assert_eq!(
        duplicate.to_http_error().status_code(),
        http::StatusCode::CONFLICT
    );

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

#[tokio::test]
async fn batch_loader_executes_one_related_query_for_many_parents() {
    use rustflow_server::sea_orm::{DbBackend, MockDatabase};

    let database = MockDatabase::new(DbBackend::Sqlite)
        .append_query_results([vec![
            entities::note::Model {
                id: 1,
                account_id: 10,
                title: "first".to_owned(),
                body: None,
            },
            entities::note::Model {
                id: 2,
                account_id: 10,
                title: "second".to_owned(),
                body: None,
            },
            entities::note::Model {
                id: 3,
                account_id: 11,
                title: "third".to_owned(),
                body: None,
            },
        ]])
        .into_connection();
    let parents = vec![
        entities::account::Model {
            id: 10,
            email: "one@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
        entities::account::Model {
            id: 11,
            email: "two@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
    ];

    let loaded = load_many(&parents, entities::note::Entity, &database)
        .await
        .expect("load related rows in one batch");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].len(), 2);
    assert_eq!(loaded[1].len(), 1);

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        1,
        "batch loading must issue one related query for the whole parent slice"
    );
}

#[tokio::test]
async fn many_to_many_loader_uses_two_queries_for_many_parents() {
    use rustflow_server::sea_orm::{DbBackend, MockDatabase};

    let database = MockDatabase::new(DbBackend::Sqlite)
        .append_query_results([vec![
            entities::account_tag::Model {
                id: 1,
                account_id: 10,
                tag_id: 20,
            },
            entities::account_tag::Model {
                id: 2,
                account_id: 10,
                tag_id: 21,
            },
            entities::account_tag::Model {
                id: 3,
                account_id: 11,
                tag_id: 21,
            },
        ]])
        .append_query_results([vec![
            entities::tag::Model {
                id: 20,
                name: "language".to_owned(),
            },
            entities::tag::Model {
                id: 21,
                name: "history".to_owned(),
            },
        ]])
        .into_connection();
    let parents = vec![
        entities::account::Model {
            id: 10,
            email: "one@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
        entities::account::Model {
            id: 11,
            email: "two@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
    ];

    let loaded = load_many_to_many(
        &parents,
        entities::tag::Entity,
        entities::account_tag::Entity,
        &database,
    )
    .await
    .expect("batch load many-to-many relation");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].len(), 2);
    assert_eq!(loaded[1].len(), 1);

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        2,
        "many-to-many loading should issue one junction and one target query for the whole batch"
    );
}
