//! Compile-time SQL model metadata for the experimental Data Mapper contract.
//!
//! This module describes Rust model fields; it does not execute SQL or prove that a
//! live database schema matches the metadata. Runtime codecs, CRUD, query execution,
//! transactions and relation loading remain future Phase 6.7 work.
//!
//! Table/column identifiers are restricted to simple ASCII identifiers and must be
//! quoted by the future dialect renderer. Values must always be bound parameters.
//!
//! Example:
//!
//! ~~~
//! #[derive(rustflow::Model)]
//! #[model(table = "user_profiles")]
//! struct UserProfile {
//!     #[model(primary_key, generated)]
//!     id: i64,
//!     #[model(column = "display_name")]
//!     name: String,
//!     #[model(default)]
//!     nickname: Option<String>,
//! }
//! ~~~
//!
//! Invalid models are rejected by the derive instead of being deferred to SQL:
//!
//! ~~~compile_fail
//! #[derive(rustflow::Model)]
//! struct MissingKey {
//!     name: String,
//! }
//! ~~~
//!
//! ~~~compile_fail
//! #[derive(rustflow::Model)]
//! struct UnsupportedType {
//!     #[model(primary_key)]
//!     id: i64,
//!     address: std::net::IpAddr,
//! }
//! ~~~
//!
//! ~~~compile_fail
//! #[derive(rustflow::Model)]
//! #[model(table = "users", unknown_option = true)]
//! struct UnknownOption {
//!     #[model(primary_key)]
//!     id: i64,
//! }
//! ~~~

/// Logical Rust type categories recognized by the metadata derive.
///
/// Recognition is metadata-only: it does not mean a codec or SQL bind implementation
/// is already available for this type on every engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustType {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    String,
    Bytes,
    Decimal,
    Uuid,
    Date,
    Time,
    Timestamp,
    TimestampWithTimeZone,
    Json,
    Enum,
}

/// Static metadata for one named model field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnMetadata {
    /// Rust struct field name.
    pub field_name: &'static str,
    /// Safe SQL identifier. The query renderer must quote it for the selected dialect.
    pub column_name: &'static str,
    /// Logical category inferred by the derive or an explicit enum marker.
    pub rust_type: RustType,
    /// Whether the field type is wrapped in a single Option.
    pub nullable: bool,
    /// Whether this field participates in the primary key.
    pub primary_key: bool,
    /// Whether the database owns the field value after a write.
    pub generated: bool,
    /// Whether the live schema declares a database default for this field.
    pub has_default: bool,
}

/// Compile-time table and column metadata for a SQL Data Mapper model.
///
/// Implementations are emitted by the Model derive. This trait does not offer
/// persistence methods and does not run schema introspection.
pub trait ModelMetadata: 'static {
    /// SQL table or view identifier.
    const TABLE_NAME: &'static str;
    /// Fields in Rust declaration order.
    const COLUMNS: &'static [ColumnMetadata];
    /// Primary key column names in declared-field order; empty only for read-only models.
    const PRIMARY_KEY_COLUMNS: &'static [&'static str];
    /// Whether this model is explicitly read-only.
    const READ_ONLY: bool;
}

#[cfg(test)]
mod tests {
    use super::{ModelMetadata, RustType};

    #[allow(dead_code)]
    enum Status {
        Active,
        Disabled,
    }

    #[allow(dead_code)]
    #[derive(crate::Model)]
    #[model(table = "user_profiles")]
    struct UserProfile {
        #[model(primary_key, generated)]
        id: i64,
        #[model(column = "display_name")]
        name: String,
        #[model(default)]
        nickname: Option<String>,
        #[model(sql_type = "enum")]
        status: Status,
    }

    #[allow(dead_code)]
    #[derive(crate::Model)]
    struct ApiToken {
        #[model(primary_key)]
        token_id: String,
    }

    #[allow(dead_code)]
    #[derive(crate::Model)]
    struct CompositeKey {
        #[model(primary_key)]
        tenant_id: i64,
        #[model(primary_key)]
        item_id: i64,
    }

    #[allow(dead_code)]
    #[derive(crate::Model)]
    #[model(read_only)]
    struct AuditView {
        created_at: String,
        description: Option<String>,
    }

    #[test]
    fn derive_publishes_table_columns_and_metadata() {
        assert_eq!(UserProfile::TABLE_NAME, "user_profiles");
        assert_eq!(UserProfile::PRIMARY_KEY_COLUMNS, &["id"]);
        assert!(!UserProfile::READ_ONLY);
        let columns = UserProfile::COLUMNS;
        assert_eq!(columns.len(), 4);
        assert_eq!(columns[0].field_name, "id");
        assert_eq!(columns[0].column_name, "id");
        assert_eq!(columns[0].rust_type, RustType::I64);
        assert!(columns[0].primary_key);
        assert!(columns[0].generated);
        assert_eq!(columns[1].column_name, "display_name");
        assert_eq!(columns[1].rust_type, RustType::String);
        assert!(!columns[1].nullable);
        assert!(columns[2].nullable);
        assert!(columns[2].has_default);
        assert_eq!(columns[2].rust_type, RustType::String);
        assert_eq!(columns[3].rust_type, RustType::Enum);
    }

    #[test]
    fn derive_defaults_table_name_to_snake_case() {
        assert_eq!(ApiToken::TABLE_NAME, "api_token");
        assert_eq!(ApiToken::PRIMARY_KEY_COLUMNS, &["token_id"]);
    }

    #[test]
    fn composite_primary_keys_preserve_declaration_order() {
        assert_eq!(CompositeKey::PRIMARY_KEY_COLUMNS, &["tenant_id", "item_id"]);
    }

    #[test]
    fn read_only_keyless_models_are_explicit() {
        assert!(AuditView::READ_ONLY);
        assert!(AuditView::PRIMARY_KEY_COLUMNS.is_empty());
        assert_eq!(AuditView::COLUMNS.len(), 2);
    }
}
