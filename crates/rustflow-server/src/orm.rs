//! SQL ORM facade for the four relational engines supported by RustFlow.
//!
//! This module re-exports SeaORM 1.1.20 as the maintained entity/query/relationship
//! foundation. It is server-only; do not add it to the browser, DOM or view crates.
//! Pair RustFlow's ModelDiagnostics derive with SeaORM's DeriveEntityModel:
//!
//! ~~~rust
//! use rustflow::ModelDiagnostics;
//! use rustflow_server::orm as sea_orm;
//! use sea_orm::entity::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
//! #[sea_orm(table_name = "user_profiles")]
//! pub struct Model {
//!     #[sea_orm(primary_key)]
//!     pub id: i64,
//!     pub display_name: String,
//!     pub nickname: Option<String>,
//! }
//!
//! #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
//! pub enum Relation {}
//!
//! impl ActiveModelBehavior for ActiveModel {}
//! ~~~
//!
//! ModelDiagnostics is validation-only. SeaORM owns and generates Entity, Model,
//! ActiveModel, columns, primary keys and query metadata. Applications use an explicit
//! DatabaseConnection/DatabaseTransaction; there is no global connection, implicit
//! lazy I/O or hidden identity map. The SQLx pool bridge is integrated in Phase 6.7.2;
//! the current DatabasePool::pool() API still returns the legacy SQLx AnyPool and must
//! not be confused with this model/metadata contract.
//!
//! ## Compile-time diagnostics
//!
//! The RustFlow validation derive rejects unsupported entity/field attributes,
//! malformed identifiers, duplicate column mappings, absent or nullable primary
//! keys, nested Option, architecture-width integers, non-named structs and
//! unsupported composite-key arity before SeaORM generates the entity.
//!
//! ~~~compile_fail
//! use rustflow::ModelDiagnostics;
//! use rustflow_server::orm as sea_orm;
//! use sea_orm::entity::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
//! #[sea_orm(table_name = "users", unknown_option = true)]
//! pub struct Model {
//!     #[sea_orm(primary_key)]
//!     pub id: i64,
//! }
//! ~~~
//!
//! ~~~compile_fail
//! use rustflow::ModelDiagnostics;
//! use rustflow_server::orm as sea_orm;
//! use sea_orm::entity::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
//! #[sea_orm(table_name = "users")]
//! pub struct Model {
//!     #[sea_orm(primary_key)]
//!     pub id: Option<i64>,
//! }
//! ~~~
//!
//! ~~~compile_fail
//! use rustflow::ModelDiagnostics;
//! use rustflow_server::orm as sea_orm;
//! use sea_orm::entity::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
//! #[sea_orm(table_name = "users")]
//! pub struct Model {
//!     #[sea_orm(primary_key)]
//!     pub id: i64,
//!     #[sea_orm(primari_key)]
//!     pub name: String,
//! }
//! ~~~
//!
//! ~~~compile_fail
//! use rustflow::ModelDiagnostics;
//! use rustflow_server::orm as sea_orm;
//! use sea_orm::entity::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
//! #[sea_orm(table_name = "users")]
//! pub struct Model {
//!     #[sea_orm(primary_key)]
//!     pub id: i64,
//!     pub address: std::net::IpAddr,
//! }
//! ~~~
//!
//! ## Runtime capabilities and limits
//!
//! The selected dependency provides query construction, per-engine SQLx drivers,
//! typed values, explicit transactions, relationship metadata and batch loaders.
//! Enabling those APIs does not yet make them part of RustFlow's supported runtime
//! contract: database-pool bridging, error normalization and exact four-engine
//! behavior are gated by Phase 6.7.2 and its integration tests. Use SQLx migrations
//! for schema changes; entity metadata does not replace the existing engine-specific
//! migrations. MongoDB remains the separate BSON/document adapter and is not exposed
//! by this SQL ORM facade.

pub use sea_orm::*;
pub use sea_orm::entity::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;

    mod user_profiles {
        use super::*;

        #[derive(Clone, Debug, PartialEq, rustflow::ModelDiagnostics, DeriveEntityModel)]
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
    }

    mod tenant_items {
        use super::*;

        #[derive(Clone, Debug, PartialEq, rustflow::ModelDiagnostics, DeriveEntityModel)]
        #[sea_orm(table_name = "tenant_items")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub tenant_id: i64,
            #[sea_orm(primary_key, auto_increment = false)]
            pub item_id: i64,
            pub name: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    #[test]
    fn seaorm_derives_publish_model_and_entity_metadata() {
        use user_profiles::{ActiveModel, Column, Entity, PrimaryKey};
        assert_eq!(Entity::default().table_name(), "user_profiles");
        assert_eq!(Column::iter().count(), 3);
        assert_eq!(PrimaryKey::iter().count(), 1);
        let active = ActiveModel {
            id: NotSet,
            display_name: Set("Ada".to_owned()),
            nickname: NotSet,
        };
        assert!(active.is_changed());
        assert!(matches!(active.id, NotSet));
        assert!(matches!(active.nickname, NotSet));
    }

    #[test]
    fn seaorm_derives_support_composite_primary_keys() {
        use tenant_items::{ActiveModel, Column, Entity, PrimaryKey};
        assert_eq!(Entity::default().table_name(), "tenant_items");
        assert_eq!(Column::iter().count(), 3);
        assert_eq!(PrimaryKey::iter().count(), 2);
        let active = ActiveModel {
            tenant_id: Set(1),
            item_id: Set(7),
            name: Set("item".to_owned()),
        };
        assert!(active.is_changed());
    }
}
