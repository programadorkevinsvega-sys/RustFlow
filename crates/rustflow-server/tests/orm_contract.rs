use std::{env, str::FromStr, time::Duration};

use rustflow_server::{
    DatabaseBackend, DatabaseConfig, DatabaseErrorKind, DatabasePool,
    orm::{
        RelationGraph, RelationLoadOptions, attach_one_to_many, detach_one_to_many, load_many,
        load_many_projected, load_many_to_many, load_one, replace_one_to_one,
    },
    sea_orm::{
        ActiveValue::{NotSet, Set, Unchanged},
        ColumnTrait, Condition, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
        QueryTrait, RelationTrait,
        sea_query::{Expr, ExprTrait, OnConflict},
    },
};

#[derive(Clone, Debug, PartialEq, rustflow_server::sea_orm::FromQueryResult)]
struct NotePreview {
    id: i64,
    account_id: i64,
    title: String,
}

mod concurrent_items {
    use rustflow_server::sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "rustflow_phase676_concurrent_items")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub token: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

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
            pub account_id: Option<i64>,
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

    pub mod signed_integer_widths {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_signed_widths")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub i8_value: i8,
            pub i16_value: i16,
            pub i32_value: i32,
            pub i64_value: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod unsigned_integer_widths {
        use rustflow_server::sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
        #[sea_orm(table_name = "rustflow_orm_unsigned_widths")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub u8_value: u8,
            pub u16_value: u16,
            pub u32_value: u32,
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
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_unsigned_widths", [])
        .await
        .expect("drop unsigned integer width contract table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_signed_widths", [])
        .await
        .expect("drop signed integer width contract table");
    database
        .execute_raw("DROP TABLE IF EXISTS rustflow_orm_constraints", [])
        .await
        .expect("drop constraint contract table");
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
            "TEXT NULL DEFAULT 'database-default'",
            "JSON NOT NULL",
            "BLOB NOT NULL",
            "INTEGER PRIMARY KEY AUTOINCREMENT",
            "TEXT NULL",
        ),
        DatabaseBackend::Postgres => (
            "BIGSERIAL PRIMARY KEY",
            "BOOLEAN NOT NULL",
            "VARCHAR(255) NULL DEFAULT 'database-default'",
            "JSONB NOT NULL",
            "BYTEA NOT NULL",
            "BIGSERIAL PRIMARY KEY",
            "TEXT NULL",
        ),
        DatabaseBackend::MySql => (
            "BIGINT AUTO_INCREMENT PRIMARY KEY",
            "BOOLEAN NOT NULL",
            "VARCHAR(255) NULL DEFAULT 'database-default'",
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
    database
        .execute_raw(
            "CREATE TABLE rustflow_orm_constraints (
                positive_value INTEGER NOT NULL CHECK (positive_value > 0)
            )",
            [],
        )
        .await
        .expect("create check-constraint contract table");
    let profiles = format!(
        "CREATE TABLE rustflow_orm_profiles (
            id {note_id},
            account_id BIGINT NULL UNIQUE,
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
            "REAL",
            "REAL",
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
    let (
        integer_id_type,
        i8_type,
        i16_type,
        i32_type,
        i64_type,
        i8_check,
        i16_check,
        i32_check,
        i64_check,
    ) = match database.backend() {
        DatabaseBackend::Sqlite => (
            "INTEGER PRIMARY KEY AUTOINCREMENT",
            "INTEGER",
            "INTEGER",
            "INTEGER",
            "INTEGER",
            " CHECK (i8_value BETWEEN -128 AND 127)",
            " CHECK (i16_value BETWEEN -32768 AND 32767)",
            " CHECK (i32_value BETWEEN -2147483648 AND 2147483647)",
            " CHECK (typeof(i64_value) = 'integer' AND i64_value BETWEEN -9223372036854775808 AND 9223372036854775807)",
        ),
        DatabaseBackend::Postgres => (
            "BIGSERIAL PRIMARY KEY",
            "\"char\"",
            "SMALLINT",
            "INTEGER",
            "BIGINT",
            "",
            "",
            "",
            "",
        ),
        DatabaseBackend::MySql => (
            "BIGINT AUTO_INCREMENT PRIMARY KEY",
            "TINYINT",
            "SMALLINT",
            "INT",
            "BIGINT",
            "",
            "",
            "",
            "",
        ),
    };
    let signed_widths = format!(
        "CREATE TABLE rustflow_orm_signed_widths (
            id {integer_id_type},
            i8_value {i8_type} NULL{i8_check},
            i16_value {i16_type} NULL{i16_check},
            i32_value {i32_type} NULL{i32_check},
            i64_value {i64_type} NULL{i64_check}
        )"
    );
    database
        .execute_raw(&signed_widths, [])
        .await
        .expect("create signed integer width contract table");

    // SeaORM 1.1.x has no PostgreSQL mapping for unsigned primitive columns.
    // Exercise SQLite's checked integer representation and MySQL/MariaDB's
    // native unsigned integer widths instead of claiming PostgreSQL parity.
    if database.backend() != DatabaseBackend::Postgres {
        let (unsigned_id_type, u8_type, u16_type, u32_type) = match database.backend() {
            DatabaseBackend::Sqlite => (
                "INTEGER PRIMARY KEY AUTOINCREMENT",
                "INTEGER NULL CHECK (u8_value BETWEEN 0 AND 255)",
                "INTEGER NULL CHECK (u16_value BETWEEN 0 AND 65535)",
                "INTEGER NULL CHECK (u32_value BETWEEN 0 AND 4294967295)",
            ),
            DatabaseBackend::MySql => (
                "BIGINT AUTO_INCREMENT PRIMARY KEY",
                "TINYINT UNSIGNED NULL",
                "SMALLINT UNSIGNED NULL",
                "INT UNSIGNED NULL",
            ),
            DatabaseBackend::Postgres => unreachable!("PostgreSQL unsigned contract is excluded"),
        };
        let unsigned_widths = format!(
            "CREATE TABLE rustflow_orm_unsigned_widths (
                id {unsigned_id_type},
                u8_value {u8_type},
                u16_value {u16_type},
                u32_value {u32_type}
            )"
        );
        database
            .execute_raw(&unsigned_widths, [])
            .await
            .expect("create unsigned integer width contract table");
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
    // SeaORM 1.1.20's SQLite codec binds and decodes Decimal/BigDecimal through f64.
    // Use 12.5, which is exactly representable in binary, for SQLite. Exercise
    // arbitrary precision only on PostgreSQL and the MySQL protocol backends.
    let (decimal, big_decimal_text) = if database.backend() == DatabaseBackend::Sqlite {
        (Decimal::new(125, 1), "12.5")
    } else {
        (Decimal::new(12_345, 2), "12345678901234567890.1234567890")
    };
    let big_decimal =
        BigDecimal::from_str(big_decimal_text).expect("valid integration decimal value");
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

        for (label, invalid) in [
            ("negative u64", "-1"),
            ("u64 upper overflow", "18446744073709551616"),
        ] {
            let sql = format!("INSERT INTO rustflow_orm_mysql_unsigned (value) VALUES ({invalid})");
            database.execute_raw(&sql, []).await.expect_err(label);
        }
    }
}

async fn exercise_integer_widths(database: &DatabasePool) {
    let signed = database.entity::<entities::signed_integer_widths::Entity>();
    let minimum = signed
        .insert(entities::signed_integer_widths::ActiveModel {
            i8_value: Set(i8::MIN),
            i16_value: Set(i16::MIN),
            i32_value: Set(i32::MIN),
            i64_value: Set(i64::MIN),
            ..Default::default()
        })
        .await
        .expect("insert signed minimum values for every supported width");
    let decoded_minimum = signed
        .find_by_id(minimum.id)
        .await
        .expect("read signed minimum values")
        .expect("signed minimum row exists");
    assert_eq!(decoded_minimum.i8_value, i8::MIN);
    assert_eq!(decoded_minimum.i16_value, i16::MIN);
    assert_eq!(decoded_minimum.i32_value, i32::MIN);
    assert_eq!(decoded_minimum.i64_value, i64::MIN);

    let maximum = signed
        .insert(entities::signed_integer_widths::ActiveModel {
            i8_value: Set(i8::MAX),
            i16_value: Set(i16::MAX),
            i32_value: Set(i32::MAX),
            i64_value: Set(i64::MAX),
            ..Default::default()
        })
        .await
        .expect("insert signed maximum values for every supported width");
    let decoded_maximum = signed
        .find_by_id(maximum.id)
        .await
        .expect("read signed maximum values")
        .expect("signed maximum row exists");
    assert_eq!(decoded_maximum.i8_value, i8::MAX);
    assert_eq!(decoded_maximum.i16_value, i16::MAX);
    assert_eq!(decoded_maximum.i32_value, i32::MAX);
    assert_eq!(decoded_maximum.i64_value, i64::MAX);

    let mut signed_overflow_cases = vec![
        (
            "i16 upper overflow",
            "INSERT INTO rustflow_orm_signed_widths (i16_value) VALUES (32768)",
        ),
        (
            "i16 lower overflow",
            "INSERT INTO rustflow_orm_signed_widths (i16_value) VALUES (-32769)",
        ),
        (
            "i32 upper overflow",
            "INSERT INTO rustflow_orm_signed_widths (i32_value) VALUES (2147483648)",
        ),
        (
            "i32 lower overflow",
            "INSERT INTO rustflow_orm_signed_widths (i32_value) VALUES (-2147483649)",
        ),
        (
            "i64 upper overflow",
            "INSERT INTO rustflow_orm_signed_widths (i64_value) VALUES (9223372036854775808)",
        ),
        (
            "i64 lower overflow",
            "INSERT INTO rustflow_orm_signed_widths (i64_value) VALUES (-9223372036854775809)",
        ),
    ];
    // PostgreSQL represents i8 through its one-byte internal "char" type.
    // Its signed boundary values are round-tripped above; the remaining
    // dialects enforce the TINYINT/SQLite CHECK overflow boundary directly.
    if database.backend() != DatabaseBackend::Postgres {
        signed_overflow_cases.extend([
            (
                "i8 upper overflow",
                "INSERT INTO rustflow_orm_signed_widths (i8_value) VALUES (128)",
            ),
            (
                "i8 lower overflow",
                "INSERT INTO rustflow_orm_signed_widths (i8_value) VALUES (-129)",
            ),
        ]);
    }
    for (label, sql) in signed_overflow_cases {
        database.execute_raw(sql, []).await.expect_err(label);
    }

    if database.backend() != DatabaseBackend::Postgres {
        let unsigned = database.entity::<entities::unsigned_integer_widths::Entity>();
        let zero = unsigned
            .insert(entities::unsigned_integer_widths::ActiveModel {
                u8_value: Set(0),
                u16_value: Set(0),
                u32_value: Set(0),
                ..Default::default()
            })
            .await
            .expect("insert unsigned zero values");
        let decoded_zero = unsigned
            .find_by_id(zero.id)
            .await
            .expect("read unsigned zero values")
            .expect("unsigned zero row exists");
        assert_eq!(decoded_zero.u8_value, 0);
        assert_eq!(decoded_zero.u16_value, 0);
        assert_eq!(decoded_zero.u32_value, 0);

        let maxima = unsigned
            .insert(entities::unsigned_integer_widths::ActiveModel {
                u8_value: Set(u8::MAX),
                u16_value: Set(u16::MAX),
                u32_value: Set(u32::MAX),
                ..Default::default()
            })
            .await
            .expect("insert unsigned maximum values for every supported width");
        let decoded_maxima = unsigned
            .find_by_id(maxima.id)
            .await
            .expect("read unsigned maximum values")
            .expect("unsigned maximum row exists");
        assert_eq!(decoded_maxima.u8_value, u8::MAX);
        assert_eq!(decoded_maxima.u16_value, u16::MAX);
        assert_eq!(decoded_maxima.u32_value, u32::MAX);

        let unsigned_overflow_cases = [
            (
                "u8 upper overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u8_value) VALUES (256)",
            ),
            (
                "u8 negative overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u8_value) VALUES (-1)",
            ),
            (
                "u16 upper overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u16_value) VALUES (65536)",
            ),
            (
                "u16 negative overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u16_value) VALUES (-1)",
            ),
            (
                "u32 upper overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u32_value) VALUES (4294967296)",
            ),
            (
                "u32 negative overflow",
                "INSERT INTO rustflow_orm_unsigned_widths (u32_value) VALUES (-1)",
            ),
        ];
        for (label, sql) in unsigned_overflow_cases {
            database.execute_raw(sql, []).await.expect_err(label);
        }
    }
}

async fn exercise_orm(database: &DatabasePool) {
    create_schema(database).await;
    exercise_integer_widths(database).await;
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

    let saved_new = accounts
        .save(entities::account::ActiveModel {
            email: Set("save-new@example.test".to_owned()),
            enabled: Set(true),
            alias: NotSet,
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        })
        .await
        .expect("save inserts a new ActiveModel");
    // SeaORM turns the returned Model into an ActiveModel with persisted
    // attributes marked Unchanged rather than Set.
    let saved_id = match saved_new.id {
        Set(id) | Unchanged(id) => id,
        NotSet => panic!("save must return the generated primary key"),
    };
    let saved_existing = accounts
        .save(entities::account::ActiveModel {
            id: Set(saved_id),
            alias: Set(Some("saved-existing".to_owned())),
            ..Default::default()
        })
        .await
        .expect("save updates an existing ActiveModel");
    assert!(matches!(
        &saved_existing.alias,
        Set(Some(alias)) | Unchanged(Some(alias)) if alias == "saved-existing"
    ));
    let deleted_saved = accounts
        .delete(saved_existing)
        .await
        .expect("delete accepts a model ActiveModel");
    assert_eq!(deleted_saved.rows_affected, 1);

    let invalid_update = accounts
        .update(entities::account::ActiveModel::default())
        .await
        .expect_err("update without a primary key must be rejected");
    assert_eq!(invalid_update.kind(), DatabaseErrorKind::InvalidModelState);
    let invalid_delete = accounts
        .delete(entities::account::ActiveModel::default())
        .await
        .expect_err("delete without a primary key must be rejected");
    assert_eq!(invalid_delete.kind(), DatabaseErrorKind::InvalidModelState);

    let invalid_foreign_key = notes
        .insert(entities::note::ActiveModel {
            account_id: Set(i64::MAX),
            title: Set("orphan note".to_owned()),
            body: NotSet,
            ..Default::default()
        })
        .await
        .expect_err("a note cannot reference a missing account");
    assert_eq!(
        invalid_foreign_key.kind(),
        DatabaseErrorKind::ForeignKeyViolation
    );
    assert_eq!(
        invalid_foreign_key.to_http_error().status_code(),
        http::StatusCode::CONFLICT
    );

    let check_placeholder = if database.backend() == DatabaseBackend::Postgres {
        "$1"
    } else {
        "?"
    };
    let check_sql = format!(
        "INSERT INTO rustflow_orm_constraints (positive_value) VALUES ({check_placeholder})"
    );
    let check_error = database
        .execute_raw(&check_sql, [0_i64.into()])
        .await
        .expect_err("non-positive values must fail the CHECK constraint");
    assert_eq!(check_error.kind(), DatabaseErrorKind::CheckViolation);
    assert_eq!(
        check_error.to_http_error().status_code(),
        http::StatusCode::UNPROCESSABLE_ENTITY
    );

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
            account_id: Set(Some(first.id)),
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

    let projected_ids: Vec<i64> = accounts
        .select()
        .select_only()
        .column_as(entities::account::Column::Id, "account_identifier")
        .filter(entities::account::Column::Id.gt(first.id - 1))
        .filter(entities::account::Column::Email.is_in([
            "grace@example.test".to_owned(),
            "ada.lovelace@example.test".to_owned(),
        ]))
        .order_by_asc(entities::account::Column::Id)
        .into_tuple()
        .all(database.orm())
        .await
        .expect("projection alias, comparison and parameterized IN query");
    assert_eq!(projected_ids, vec![first.id, second.id]);

    let null_aliases = accounts
        .all(
            accounts
                .select()
                .filter(entities::account::Column::Alias.is_null()),
        )
        .await
        .expect("IS NULL query");
    assert!(!null_aliases.is_empty());

    let second_page = accounts
        .all(
            accounts
                .select()
                .order_by_asc(entities::account::Column::Id)
                .offset(1)
                .limit(1),
        )
        .await
        .expect("offset pagination query");
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].id, second.id);

    let joined = entities::account::Entity::find()
        .join(JoinType::LeftJoin, entities::account::Relation::Notes.def())
        .distinct()
        .all(database.orm())
        .await
        .expect("left join query");
    assert_eq!(joined.len(), 2);

    accounts
        .insert(entities::account::ActiveModel {
            email: Set("empty@example.test".to_owned()),
            enabled: Set(false),
            alias: Set(None),
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        })
        .await
        .expect("insert parent without any related rows");

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
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].len(), 2);
    assert_eq!(children[1].len(), 1);
    assert!(children[2].is_empty(), "parent has no has-many children");
    // Apply relation-specific filters and deterministic ordering inside the batch loader.
    let filtered_children = load_many(
        &parents,
        RelationLoadOptions::<entities::note::Entity>::new().modify(|query| {
            query
                .filter(entities::note::Column::Title.contains("note"))
                .order_by_desc(entities::note::Column::Id)
        }),
        database.orm(),
    )
    .await
    .expect("batch load filtered and ordered notes");
    assert_eq!(filtered_children.len(), parents.len());
    assert_eq!(
        filtered_children[0]
            .iter()
            .map(|note| note.title.as_str())
            .collect::<Vec<_>>(),
        vec!["second note", "first note"],
        "the related query order must be preserved within each parent's group"
    );
    assert_eq!(filtered_children[1][0].title, "third note");
    assert!(filtered_children[2].is_empty());

    // A projection DTO includes the foreign key used to group results. The body column
    // is deliberately omitted, so this verifies that the loader does not decode a
    // partial row into the full Note model.
    let projected_notes: Vec<Vec<NotePreview>> = load_many_projected(
        &parents,
        entities::note::Column::AccountId,
        RelationLoadOptions::<entities::note::Entity>::new().modify(|query| {
            query
                .select_only()
                .column(entities::note::Column::Id)
                .column(entities::note::Column::AccountId)
                .column(entities::note::Column::Title)
                .order_by_desc(entities::note::Column::Id)
        }),
        |account| account.id,
        |note: &NotePreview| note.account_id,
        database.orm(),
    )
    .await
    .expect("batch load a safe DTO projection");
    assert_eq!(projected_notes.len(), parents.len());
    assert_eq!(projected_notes[0].len(), 2);
    assert_eq!(projected_notes[0][0].title, "second note");
    assert!(projected_notes[0][0].id > projected_notes[0][1].id);
    assert_eq!(projected_notes[1][0].title, "third note");
    assert!(projected_notes[2].is_empty());

    // A configured graph supports multiple branches and nested branches while retaining
    // explicit loaders. Each graph edge is a batch operation, not a query per parent.
    let note_graph = RelationGraph::<entities::note::Model, _>::new()
        .with_one(
            "author",
            RelationLoadOptions::<entities::account::Entity>::new(),
            RelationGraph::new(),
        )
        .expect("configure nested author relation");
    let graph = RelationGraph::<entities::account::Model, _>::new()
        .with_many(
            "notes",
            RelationLoadOptions::<entities::note::Entity>::new()
                .modify(|query| query.order_by_desc(entities::note::Column::Id)),
            note_graph,
        )
        .expect("configure notes graph branch")
        .with_one(
            "profile",
            RelationLoadOptions::<entities::profile::Entity>::new(),
            RelationGraph::new(),
        )
        .expect("configure profile graph branch")
        .with_many_to_many(
            "tags",
            RelationLoadOptions::<entities::tag::Entity>::new(),
            entities::account_tag::Entity,
            RelationGraph::new(),
        )
        .expect("configure tags graph branch")
        .with_many_projected(
            "note_summaries",
            entities::note::Column::AccountId,
            RelationLoadOptions::<entities::note::Entity>::new().modify(|query| {
                query
                    .select_only()
                    .column(entities::note::Column::Id)
                    .column(entities::note::Column::AccountId)
                    .column(entities::note::Column::Title)
                    .order_by_desc(entities::note::Column::Id)
            }),
            |account| account.id,
            |note: &NotePreview| note.account_id,
        )
        .expect("configure projected note graph branch")
        .with_max_depth(3)
        .expect("set graph depth");
    let graph_results = graph
        .load(&parents, database.orm())
        .await
        .expect("execute explicit nested relation graph");
    let graph_notes = graph_results
        .many::<entities::note::Model>("notes")
        .expect("read graph notes");
    assert_eq!(graph_notes.len(), parents.len());
    assert_eq!(graph_notes[0][0].title, "second note");
    assert_eq!(graph_notes[1][0].title, "third note");
    let graph_authors = graph_results
        .one::<entities::account::Model>("notes.author")
        .expect("read nested graph authors");
    assert_eq!(graph_authors.len(), 3);
    assert!(
        graph_results
            .one::<entities::profile::Model>("profile")
            .is_some()
    );
    assert_eq!(
        graph_results
            .many::<entities::tag::Model>("tags")
            .unwrap()
            .len(),
        parents.len()
    );
    assert_eq!(
        graph_results.many::<NotePreview>("note_summaries").unwrap()[0][0].title,
        "second note"
    );

    // Reassigning a has-many child is an explicit typed update and participates in the
    // transaction supplied by the caller. A rollback restores the original parent key.
    let third_note = notes
        .one(
            notes
                .select()
                .filter(entities::note::Column::Title.eq("third note")),
        )
        .await
        .expect("find note to move")
        .expect("third note exists");
    let third_note_id = third_note.id;
    let mutation = database
        .begin_transaction()
        .await
        .expect("begin relation mutation");
    let moved = attach_one_to_many(
        third_note,
        entities::note::Column::AccountId,
        first.id.into(),
        mutation.connection(),
    )
    .await
    .expect("move a has-many child to another parent");
    assert_eq!(moved.account_id, first.id);
    mutation
        .rollback()
        .await
        .expect("rollback relation reparenting");
    let restored_note = notes
        .find_by_id(third_note_id)
        .await
        .expect("reload rolled-back note")
        .expect("note remains present");
    assert_eq!(restored_note.account_id, second.id);

    // One-to-one replacement explicitly detaches the old nullable FK and attaches the
    // replacement inside one transaction; no related row is deleted.
    let profile_repo = database.entity::<entities::profile::Entity>();
    let old_profile = profile_repo
        .one(
            profile_repo
                .select()
                .filter(entities::profile::Column::AccountId.eq(Some(first.id))),
        )
        .await
        .expect("find existing profile")
        .expect("profile exists");
    let replacement_profile = profile_repo
        .insert(entities::profile::ActiveModel {
            account_id: Set(None),
            bio: Set("Replacement profile".to_owned()),
            ..Default::default()
        })
        .await
        .expect("create unassociated replacement profile");
    let replacement = database
        .begin_transaction()
        .await
        .expect("begin one-to-one replacement");
    let replaced_profile = replace_one_to_one(
        Some(old_profile),
        replacement_profile,
        entities::profile::Column::AccountId,
        first.id.into(),
        &replacement,
    )
    .await
    .expect("replace one-to-one profile");
    assert_eq!(replaced_profile.bio, "Replacement profile");
    replacement
        .commit()
        .await
        .expect("commit one-to-one replacement");
    let profiles_after_replace = load_one(&parents, entities::profile::Entity, database.orm())
        .await
        .expect("reload one-to-one profiles");
    assert_eq!(
        profiles_after_replace[0]
            .as_ref()
            .map(|profile| profile.bio.as_str()),
        Some("Replacement profile")
    );
    assert!(profiles_after_replace[1].is_none());

    let all_notes = notes
        .all(notes.select().order_by_asc(entities::note::Column::Id))
        .await
        .expect("load notes for belongs-to relationship");
    let note_accounts = load_one(&all_notes, entities::account::Entity, database.orm())
        .await
        .expect("batch load belongs-to accounts");
    assert_eq!(note_accounts.len(), 3);
    let note_accounts: Vec<_> = note_accounts
        .into_iter()
        .map(|account| account.expect("each note has a parent account"))
        .collect();
    assert_eq!(note_accounts[0].id, first.id);
    assert_eq!(note_accounts[1].id, first.id);
    assert_eq!(note_accounts[2].id, second.id);

    // Compose a second explicit loader hop to demonstrate nested traversal without lazy I/O.
    let nested_profiles = load_one(&note_accounts, entities::profile::Entity, database.orm())
        .await
        .expect("explicitly load profiles for previously loaded note accounts");
    assert_eq!(
        nested_profiles[0]
            .as_ref()
            .map(|profile| profile.bio.as_str()),
        Some("Replacement profile")
    );
    assert_eq!(
        nested_profiles[1]
            .as_ref()
            .map(|profile| profile.bio.as_str()),
        Some("Replacement profile")
    );
    assert!(nested_profiles[2].is_none());

    let profiles = load_one(&parents, entities::profile::Entity, database.orm())
        .await
        .expect("batch load one-to-one relation");
    assert_eq!(profiles.len(), 3);
    assert_eq!(
        profiles[0].as_ref().map(|profile| profile.bio.as_str()),
        Some("Replacement profile")
    );
    assert!(profiles[1].is_none());
    assert!(profiles[2].is_none());

    // Detach only after the relationship loaders have asserted the replacement.
    // The nullable FK becomes NULL, but the related row remains persisted.
    let replacement_for_detach = profiles[0]
        .clone()
        .expect("replacement profile is associated");
    let detached_profile_id = replacement_for_detach.id;
    let detach_transaction = database
        .begin_transaction()
        .await
        .expect("begin explicit one-to-one detach");
    let detached_profile = detach_one_to_many(
        replacement_for_detach,
        entities::profile::Column::AccountId,
        detach_transaction.connection(),
    )
    .await
    .expect("detach a nullable one-to-one foreign key without deleting the row");
    assert!(detached_profile.account_id.is_none());
    detach_transaction
        .commit()
        .await
        .expect("commit explicit one-to-one detach");
    let persisted_detached_profile = profile_repo
        .find_by_id(detached_profile_id)
        .await
        .expect("query detached profile")
        .expect("detaching a relation must not delete the profile");
    assert!(persisted_detached_profile.account_id.is_none());
    let profiles_after_detach = load_one(&parents, entities::profile::Entity, database.orm())
        .await
        .expect("reload detached profiles");
    assert!(profiles_after_detach[0].is_none());

    let loaded_tags = load_many_to_many(
        &parents,
        entities::tag::Entity,
        entities::account_tag::Entity,
        database.orm(),
    )
    .await
    .expect("batch load many-to-many tags");
    assert_eq!(loaded_tags.len(), 3);
    assert_eq!(loaded_tags[0].len(), 2);
    assert_eq!(loaded_tags[1].len(), 1);
    assert!(loaded_tags[2].is_empty());
    assert_eq!(loaded_tags[0][0].id, first_tag.id);
    assert_eq!(loaded_tags[0][1].id, second_tag.id);

    let relation_transaction = database
        .begin_transaction()
        .await
        .expect("begin relation mutation transaction");
    let relation_delete = relation_transaction
        .entity::<entities::account_tag::Entity>()
        .delete_many()
        .filter(entities::account_tag::Column::AccountId.eq(first.id))
        .exec(relation_transaction.connection())
        .await
        .expect("remove junction rows inside transaction");
    assert_eq!(relation_delete.rows_affected, 2);
    relation_transaction
        .rollback()
        .await
        .expect("rollback relation mutation");

    // Exercise the complete explicit many-to-many mutation lifecycle:
    // attach a junction row, change its target, detach it, then verify that
    // deleting a related tag cascades to its junction row.
    let tags = database.entity::<entities::tag::Entity>();
    let mutation_tag = tags
        .insert(entities::tag::ActiveModel {
            name: Set("mutation-target".to_owned()),
            ..Default::default()
        })
        .await
        .expect("create relation mutation target");
    let attached = account_tags
        .insert(entities::account_tag::ActiveModel {
            account_id: Set(second.id),
            tag_id: Set(mutation_tag.id),
            ..Default::default()
        })
        .await
        .expect("attach a tag through a junction row");
    assert_eq!(attached.account_id, second.id);
    assert_eq!(attached.tag_id, mutation_tag.id);

    let updated_link = account_tags
        .update(entities::account_tag::ActiveModel {
            id: Set(attached.id),
            tag_id: Set(first_tag.id),
            ..Default::default()
        })
        .await
        .expect("update the related target through its junction row");
    assert_eq!(updated_link.account_id, second.id);
    assert_eq!(updated_link.tag_id, first_tag.id);

    let detached = account_tags
        .delete_by_id(attached.id)
        .await
        .expect("detach the updated junction row");
    assert_eq!(detached.rows_affected, 1);
    let detached_link = account_tags
        .one(
            account_tags
                .select()
                .filter(entities::account_tag::Column::Id.eq(attached.id)),
        )
        .await
        .expect("verify detached junction row");
    assert!(detached_link.is_none());

    let cascade_tag = tags
        .insert(entities::tag::ActiveModel {
            name: Set("cascade-target".to_owned()),
            ..Default::default()
        })
        .await
        .expect("create tag for cascade-delete test");
    let cascade_link = account_tags
        .insert(entities::account_tag::ActiveModel {
            account_id: Set(first.id),
            tag_id: Set(cascade_tag.id),
            ..Default::default()
        })
        .await
        .expect("attach cascade-delete tag");
    let deleted_tag = tags
        .delete_by_id(cascade_tag.id)
        .await
        .expect("delete related tag");
    assert_eq!(deleted_tag.rows_affected, 1);
    let cascaded_link = account_tags
        .one(
            account_tags
                .select()
                .filter(entities::account_tag::Column::Id.eq(cascade_link.id)),
        )
        .await
        .expect("verify foreign-key cascade removed junction row");
    assert!(cascaded_link.is_none());

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
        .expect("insert ORM model inside transaction");
    let raw_transaction_insert = match database.backend() {
        DatabaseBackend::Postgres => {
            "INSERT INTO rustflow_orm_notes (account_id, title, body) VALUES ($1, $2, NULL)"
        }
        DatabaseBackend::Sqlite | DatabaseBackend::MySql => {
            "INSERT INTO rustflow_orm_notes (account_id, title, body) VALUES (?, ?, NULL)"
        }
    };
    transaction
        .execute_raw(
            raw_transaction_insert,
            [first.id.into(), "raw rolled back".into()],
        )
        .await
        .expect("insert raw SQL inside the same transaction");
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
    let raw_rolled_back = notes
        .one(
            notes
                .select()
                .filter(entities::note::Column::Title.eq("raw rolled back")),
        )
        .await
        .expect("query raw SQL rollback result");
    assert!(raw_rolled_back.is_none());

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
        .expect("insert ORM model inside committed transaction");
    commit
        .execute_raw(
            raw_transaction_insert,
            [first.id.into(), "raw committed".into()],
        )
        .await
        .expect("insert raw SQL inside the same committed transaction");
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
    let raw_committed = notes
        .one(
            notes
                .select()
                .filter(entities::note::Column::Title.eq("raw committed")),
        )
        .await
        .expect("query raw SQL commit result");
    assert!(raw_committed.is_some());

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
    assert_eq!(duplicate.kind(), DatabaseErrorKind::UniqueViolation);
    assert_eq!(
        duplicate.to_http_error().status_code(),
        http::StatusCode::CONFLICT
    );

    let removed = accounts
        .delete_by_id(second.id)
        .await
        .expect("delete account by id");
    assert_eq!(removed.rows_affected, 1);

    let defaults = accounts
        .insert(entities::account::ActiveModel {
            email: Set("defaults@example.test".to_owned()),
            enabled: Set(true),
            alias: NotSet,
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        })
        .await
        .expect("database default should populate an omitted field");
    assert_eq!(defaults.alias.as_deref(), Some("database-default"));

    let capabilities = database.capabilities();
    assert!(capabilities.upsert);
    assert_eq!(
        capabilities.insert_returning,
        matches!(
            database.backend(),
            DatabaseBackend::Sqlite | DatabaseBackend::Postgres
        )
    );

    let upsert_model = |enabled| entities::account::ActiveModel {
        email: Set("upsert@example.test".to_owned()),
        enabled: Set(enabled),
        alias: NotSet,
        metadata: Set(serde_json::json!({})),
        checksum: Set(vec![]),
        ..Default::default()
    };
    accounts
        .insert_builder(upsert_model(true))
        .on_conflict(
            OnConflict::column(entities::account::Column::Email)
                .update_columns([entities::account::Column::Enabled])
                .to_owned(),
        )
        .exec(database.orm())
        .await
        .expect("initial dialect-aware upsert insert");
    accounts
        .insert_builder(upsert_model(false))
        .on_conflict(
            OnConflict::column(entities::account::Column::Email)
                .update_columns([entities::account::Column::Enabled])
                .to_owned(),
        )
        .exec(database.orm())
        .await
        .expect("repeat dialect-aware upsert update");
    let upserted = accounts
        .one(
            accounts
                .select()
                .filter(entities::account::Column::Email.eq("upsert@example.test")),
        )
        .await
        .expect("query dialect-aware upsert result")
        .expect("upserted account exists");
    assert!(!upserted.enabled);

    let missing_update = accounts
        .update(entities::account::ActiveModel {
            id: Set(i64::MAX),
            email: Set("missing@example.test".to_owned()),
            enabled: Set(true),
            alias: NotSet,
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
        })
        .await
        .expect_err("updating a missing primary key must report not found");
    assert_eq!(missing_update.kind(), DatabaseErrorKind::NotFound);
    assert_eq!(
        missing_update.to_http_error().status_code(),
        http::StatusCode::NOT_FOUND
    );

    let missing_delete = accounts
        .delete_by_id(i64::MAX)
        .await
        .expect("deleting a missing key is a zero-row result");
    assert_eq!(missing_delete.rows_affected, 0);

    let cleanup = accounts
        .delete_by_id(defaults.id)
        .await
        .expect("cleanup default row");
    assert_eq!(cleanup.rows_affected, 1);

    // Query expressions must bind hostile-looking values as data, never SQL fragments.
    let hostile_email = "attacker@example.test' OR 1=1; DROP TABLE rustflow_orm_accounts; --";
    let hostile_account = accounts
        .insert(entities::account::ActiveModel {
            email: Set(hostile_email.to_owned()),
            enabled: Set(true),
            alias: Set(None),
            metadata: Set(serde_json::json!({})),
            checksum: Set(vec![]),
            ..Default::default()
        })
        .await
        .expect("hostile-looking input is stored as a bound value");
    let exact_match = accounts
        .one(
            accounts
                .select()
                .filter(entities::account::Column::Email.eq(hostile_email)),
        )
        .await
        .expect("hostile-looking input is queried as a bound value")
        .expect("exact hostile-looking value is present");
    assert_eq!(exact_match.id, hostile_account.id);
    let surviving_rows = accounts
        .all(accounts.select())
        .await
        .expect("account table remains queryable after hostile-looking input");
    assert!(
        surviving_rows
            .iter()
            .any(|row| row.id == hostile_account.id)
    );
    accounts
        .delete_by_id(hostile_account.id)
        .await
        .expect("remove query-safety fixture");
}

async fn assert_concurrent_writes_are_isolated_and_complete(url: &str) {
    let maximum_connections = if url.starts_with("sqlite:") { 1 } else { 4 };
    let config = DatabaseConfig::new(url)
        .expect("valid concurrent-write test URL")
        .with_max_connections(maximum_connections)
        .expect("valid concurrent-write pool size");
    let database = DatabasePool::connect(&config)
        .await
        .expect("connect concurrent-write test database");

    database
        .execute_raw(
            "DROP TABLE IF EXISTS rustflow_phase676_concurrent_items",
            [],
        )
        .await
        .expect("remove stale concurrent-write fixture");
    let create_table = if url.starts_with("sqlite:") {
        "CREATE TABLE rustflow_phase676_concurrent_items (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL UNIQUE)"
    } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        "CREATE TABLE rustflow_phase676_concurrent_items (id BIGSERIAL PRIMARY KEY, token TEXT NOT NULL UNIQUE)"
    } else {
        "CREATE TABLE rustflow_phase676_concurrent_items (id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY, token VARCHAR(255) NOT NULL UNIQUE) ENGINE=InnoDB"
    };
    database
        .execute_raw(create_table, [])
        .await
        .expect("create concurrent-write fixture");

    let writes = (0..32).map(|index| {
        let database = database.clone();
        tokio::spawn(async move {
            database
                .entity::<concurrent_items::Entity>()
                .insert(concurrent_items::ActiveModel {
                    token: Set(format!("concurrent-{index}@phase676.test")),
                    ..Default::default()
                })
                .await
        })
    });

    for write in writes {
        write
            .await
            .expect("concurrent writer task must not panic")
            .expect("concurrent ORM write must succeed");
    }

    let repository = database.entity::<concurrent_items::Entity>();
    let rows = repository
        .all(repository.select())
        .await
        .expect("read concurrent-write results");
    assert_eq!(
        rows.len(),
        32,
        "all distinct concurrent writes must be visible"
    );

    database
        .execute_raw("DROP TABLE rustflow_phase676_concurrent_items", [])
        .await
        .expect("drop concurrent-write fixture");
    database.close().await;
}

#[tokio::test]
async fn sqlite_concurrent_writes_complete_without_lost_rows() {
    assert_concurrent_writes_are_isolated_and_complete("sqlite::memory:").await;
}

#[tokio::test]
async fn postgres_concurrent_writes_complete_without_lost_rows() {
    let Some(url) = integration_url("RUSTFLOW_TEST_DATABASE_URL") else {
        return;
    };
    assert_concurrent_writes_are_isolated_and_complete(&url).await;
}

#[tokio::test]
async fn mysql_concurrent_writes_complete_without_lost_rows() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MYSQL_URL") else {
        return;
    };
    assert_concurrent_writes_are_isolated_and_complete(&url).await;
}

#[tokio::test]
async fn mariadb_concurrent_writes_complete_without_lost_rows() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MARIADB_URL") else {
        return;
    };
    assert_concurrent_writes_are_isolated_and_complete(&url).await;
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

async fn assert_pool_exhaustion_is_bounded_and_classified(url: &str) {
    let config = DatabaseConfig::new(url)
        .expect("valid pool-exhaustion test URL")
        .with_max_connections(1)
        .expect("single-connection pool")
        .with_acquire_timeout(Duration::from_millis(150))
        .expect("bounded acquire timeout");
    let database = DatabasePool::connect(&config)
        .await
        .expect("connect pool-exhaustion test database");

    // Holding the only pooled connection makes the following operation exercise the
    // real acquire-timeout path instead of a synthetic driver error.
    let transaction = database
        .begin_transaction()
        .await
        .expect("acquire the only connection");
    let error = tokio::time::timeout(Duration::from_secs(2), database.query_raw("SELECT 1", []))
    .await
    .expect("pool exhaustion must be bounded")
    .expect_err("a second operation must not exceed the configured pool size");
    assert_eq!(
        error.kind(),
        DatabaseErrorKind::Unavailable,
        "pool-acquire timeout must have the stable unavailable classification: {error:?}"
    );

    transaction
        .rollback()
        .await
        .expect("release the only pooled connection");
    database
        .query_raw("SELECT 1", [])
        .await
        .expect("pool remains usable after the connection is released");
    database.close().await;
}

#[tokio::test]
async fn sqlite_pool_exhaustion_is_bounded_and_classified() {
    assert_pool_exhaustion_is_bounded_and_classified("sqlite::memory:").await;
}

#[tokio::test]
async fn postgres_pool_exhaustion_is_bounded_and_classified() {
    let Some(url) = integration_url("RUSTFLOW_TEST_DATABASE_URL") else {
        return;
    };
    assert_pool_exhaustion_is_bounded_and_classified(&url).await;
}

#[tokio::test]
async fn mysql_pool_exhaustion_is_bounded_and_classified() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MYSQL_URL") else {
        return;
    };
    assert_pool_exhaustion_is_bounded_and_classified(&url).await;
}

#[tokio::test]
async fn mariadb_pool_exhaustion_is_bounded_and_classified() {
    let Some(url) = integration_url("RUSTFLOW_TEST_MARIADB_URL") else {
        return;
    };
    assert_pool_exhaustion_is_bounded_and_classified(&url).await;
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
        entities::account::Model {
            id: 12,
            email: "three@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
    ];

    let loaded = load_many(&parents, entities::note::Entity, &database)
        .await
        .expect("load related rows in one batch");
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].len(), 2);
    assert_eq!(loaded[1].len(), 1);
    assert!(loaded[2].is_empty());

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        1,
        "batch loading must issue one related query for the whole parent slice, including parents without children"
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
        entities::account::Model {
            id: 12,
            email: "three@example.test".to_owned(),
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
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].len(), 2);
    assert_eq!(loaded[1].len(), 1);
    assert!(loaded[2].is_empty());

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        2,
        "many-to-many loading should issue one junction and one target query for the whole batch"
    );
}

#[tokio::test]
async fn nested_relation_loading_uses_one_query_per_explicit_hop() {
    use rustflow_server::sea_orm::{DbBackend, MockDatabase};

    let database = MockDatabase::new(DbBackend::Sqlite)
        .append_query_results([vec![
            entities::note::Model {
                id: 1,
                account_id: 10,
                title: "first hop".to_owned(),
                body: None,
            },
            entities::note::Model {
                id: 2,
                account_id: 99,
                title: "missing owner".to_owned(),
                body: None,
            },
        ]])
        .append_query_results([vec![entities::account::Model {
            id: 10,
            email: "present@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        }]])
        .into_connection();
    let parents = vec![
        entities::account::Model {
            id: 10,
            email: "present@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
        entities::account::Model {
            id: 99,
            email: "missing@example.test".to_owned(),
            enabled: true,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
    ];

    let notes = load_many(&parents, entities::note::Entity, &database)
        .await
        .expect("first explicit has-many hop");
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].len(), 1);
    assert_eq!(notes[1].len(), 1);

    let nested_parents: Vec<_> = notes.into_iter().flatten().collect();
    let owners = load_one(&nested_parents, entities::account::Entity, &database)
        .await
        .expect("second explicit belongs-to hop");
    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0].as_ref().map(|owner| owner.id), Some(10));
    assert!(owners[1].is_none(), "missing belongs-to rows remain None");

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        2,
        "a two-hop graph must use one batched query per explicit hop, never one query per parent"
    );
}

#[tokio::test]
async fn configured_relation_graph_batches_each_nested_hop() {
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
                account_id: 11,
                title: "second".to_owned(),
                body: None,
            },
        ]])
        .append_query_results([vec![
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

    let graph = RelationGraph::<entities::account::Model, _>::new()
        .with_many(
            "notes",
            RelationLoadOptions::<entities::note::Entity>::new()
                .modify(|query| query.filter(entities::note::Column::Title.contains("s"))),
            RelationGraph::<entities::note::Model, _>::new()
                .with_one(
                    "author",
                    RelationLoadOptions::<entities::account::Entity>::new(),
                    RelationGraph::new(),
                )
                .expect("configure nested author"),
        )
        .expect("configure graph");
    let results = graph
        .load(&parents, &database)
        .await
        .expect("execute configured graph");
    assert_eq!(
        results
            .many::<entities::note::Model>("notes")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        results
            .one::<entities::account::Model>("notes.author")
            .unwrap()
            .len(),
        2
    );

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        2,
        "a two-edge graph must execute one batched query per edge, regardless of parent count"
    );
}

#[tokio::test]
async fn projected_relation_loading_preserves_parent_grouping_and_one_query() {
    use rustflow_server::sea_orm::{DbBackend, MockDatabase};

    let database = MockDatabase::new(DbBackend::Sqlite)
        .append_query_results([vec![
            entities::note::Model {
                id: 2,
                account_id: 10,
                title: "second".to_owned(),
                body: None,
            },
            entities::note::Model {
                id: 1,
                account_id: 10,
                title: "first".to_owned(),
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
        entities::account::Model {
            id: 12,
            email: "empty@example.test".to_owned(),
            enabled: false,
            alias: None,
            metadata: serde_json::json!({}),
            checksum: vec![],
        },
    ];

    let loaded: Vec<Vec<NotePreview>> = load_many_projected(
        &parents,
        entities::note::Column::AccountId,
        RelationLoadOptions::<entities::note::Entity>::new().modify(|query| {
            query
                .select_only()
                .column(entities::note::Column::Id)
                .column(entities::note::Column::AccountId)
                .column(entities::note::Column::Title)
                .order_by_desc(entities::note::Column::Id)
        }),
        |parent| parent.id,
        |note: &NotePreview| note.account_id,
        &database,
    )
    .await
    .expect("execute projected batch load");

    assert_eq!(loaded.len(), 3);
    assert_eq!(
        loaded[0].iter().map(|note| note.id).collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(loaded[1][0].title, "third");
    assert!(loaded[2].is_empty());

    let query_log = database.into_transaction_log();
    assert_eq!(
        query_log.len(),
        1,
        "projected relationship loading must issue one query for the entire parent slice"
    );
}

#[tokio::test]
async fn postgres_commit_transport_loss_is_reported_as_unknown() {
    use std::time::{Duration, Instant};

    let Some(url) = integration_url("RUSTFLOW_TEST_DATABASE_URL") else {
        return;
    };

    let database = database_from_url(&url).await;
    let admin = rustflow_server::sqlx::PgPool::connect(&url)
        .await
        .expect("connect PostgreSQL fault-injection control pool");

    rustflow_server::sqlx::query("DROP TABLE IF EXISTS rustflow_orm_commit_fault")
        .execute(&admin)
        .await
        .expect("remove prior commit fault table");
    rustflow_server::sqlx::query("DROP FUNCTION IF EXISTS rustflow_orm_wait_before_commit()")
        .execute(&admin)
        .await
        .expect("remove prior commit fault trigger function");
    rustflow_server::sqlx::query("CREATE TABLE rustflow_orm_commit_fault (value INTEGER NOT NULL)")
        .execute(&admin)
        .await
        .expect("create commit fault table");
    rustflow_server::sqlx::query(
        r#"
        CREATE FUNCTION rustflow_orm_wait_before_commit() RETURNS trigger
        LANGUAGE plpgsql AS $function$
        BEGIN
            PERFORM pg_sleep(30);
            RETURN NEW;
        END;
        $function$
        "#,
    )
    .execute(&admin)
    .await
    .expect("create deferred commit trigger function");
    rustflow_server::sqlx::query(
        r#"
        CREATE CONSTRAINT TRIGGER rustflow_orm_commit_fault_trigger
        AFTER INSERT ON rustflow_orm_commit_fault
        DEFERRABLE INITIALLY DEFERRED
        FOR EACH ROW
        EXECUTE FUNCTION rustflow_orm_wait_before_commit()
        "#,
    )
    .execute(&admin)
    .await
    .expect("create deferred trigger that pauses inside COMMIT");

    let transaction = database
        .begin_transaction()
        .await
        .expect("begin transaction for live COMMIT fault injection");
    let pid_rows = transaction
        .query_raw("SELECT pg_backend_pid() AS backend_pid", [])
        .await
        .expect("read PostgreSQL backend PID inside transaction");
    let backend_pid: i32 = pid_rows
        .first()
        .expect("backend PID query returns one row")
        .try_get("", "backend_pid")
        .expect("decode PostgreSQL backend PID");
    transaction
        .execute_raw(
            "INSERT INTO rustflow_orm_commit_fault (value) VALUES (1)",
            [],
        )
        .await
        .expect("insert row whose deferred trigger runs during COMMIT");

    let commit_task = tokio::spawn(async move { transaction.commit().await });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut observed_commit_wait = false;
    while Instant::now() < deadline {
        let in_commit_trigger = rustflow_server::sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_stat_activity
                WHERE pid = $1
                  AND state = 'active'
                  AND upper(query) LIKE 'COMMIT%'
                  AND wait_event = 'PgSleep'
            )",
        )
        .bind(backend_pid)
        .fetch_one(&admin)
        .await
        .expect("inspect PostgreSQL commit backend activity");
        if in_commit_trigger {
            observed_commit_wait = true;
            break;
        }
        if commit_task.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    if !observed_commit_wait {
        let _ = rustflow_server::sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(backend_pid)
            .fetch_one(&admin)
            .await;
        let _ = commit_task.await;
        panic!("PostgreSQL COMMIT never reached the deferred-trigger PgSleep fault window");
    }

    let terminated =
        rustflow_server::sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(backend_pid)
            .fetch_one(&admin)
            .await
            .expect("terminate the backend while COMMIT is executing");
    assert!(
        terminated,
        "PostgreSQL must terminate the active COMMIT backend"
    );

    let commit_result = tokio::time::timeout(Duration::from_secs(10), commit_task)
        .await
        .expect("commit task must finish promptly after its backend is terminated")
        .expect("commit task must not panic after transport loss");
    let commit_error =
        commit_result.expect_err("a transport drop during COMMIT must not be reported as success");
    assert_eq!(
        commit_error.kind(),
        DatabaseErrorKind::CommitOutcomeUnknown,
        "a lost PostgreSQL connection while COMMIT is in flight has an unknown client-visible outcome"
    );

    rustflow_server::sqlx::query("DROP TABLE IF EXISTS rustflow_orm_commit_fault")
        .execute(&admin)
        .await
        .expect("clean up commit fault table");
    rustflow_server::sqlx::query("DROP FUNCTION IF EXISTS rustflow_orm_wait_before_commit()")
        .execute(&admin)
        .await
        .expect("clean up deferred commit trigger function");
    admin.close().await;
    database.close().await;
}
