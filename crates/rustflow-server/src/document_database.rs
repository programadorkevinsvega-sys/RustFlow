//! A document-oriented MongoDB adapter kept separate from SQLx and SQL migrations.
//!
//! The adapter intentionally exposes the official driver's collection API rather than
//! introducing an ODM, query builder, or relational abstraction.

use std::{
    error::Error as StdError,
    fmt,
    future::Future,
    time::Duration,
};

use http::StatusCode;
use crate::mongodb_driver::{
    bson::{self, doc, Document},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Database, IndexModel,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::{AppConfig, HttpError};

/// MongoDB connection settings.
///
/// The URI may contain credentials and is deliberately redacted in Debug output.
/// Operations passed to MongoDatabase::run_with_timeout are bounded by the configured
/// operation timeout.
#[derive(Clone, PartialEq, Eq)]
pub struct MongoConfig {
    uri: String,
    database: String,
    connect_timeout: Duration,
    server_selection_timeout: Duration,
    operation_timeout: Duration,
}

impl fmt::Debug for MongoConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MongoConfig")
            .field("uri", &"[REDACTED]")
            .field("database", &self.database)
            .field("connect_timeout", &self.connect_timeout)
            .field("server_selection_timeout", &self.server_selection_timeout)
            .field("operation_timeout", &self.operation_timeout)
            .finish()
    }
}

impl MongoConfig {
    /// Validate a MongoDB URI and database name, using conservative timeouts.
    pub fn new(uri: impl Into<String>, database: impl Into<String>) -> Result<Self, MongoError> {
        let uri = uri.into();
        let database = database.into();
        if uri.is_empty()
            || uri.chars().any(char::is_control)
            || !(uri.starts_with("mongodb://") || uri.starts_with("mongodb+srv://"))
            || uri.ends_with("://")
        {
            return Err(MongoError::InvalidConfiguration(
                "MongoDB URI must use mongodb:// or mongodb+srv:// and contain no control characters",
            ));
        }
        validate_database_name(&database)?;

        Ok(Self {
            uri,
            database,
            connect_timeout: Duration::from_secs(10),
            server_selection_timeout: Duration::from_secs(10),
            operation_timeout: Duration::from_secs(15),
        })
    }

    /// Resolve settings from the application's normal configuration sources.
    ///
    /// In production, AppConfig resolves process-environment values only. In
    /// development/test, its normal dotenv precedence applies. Required variables:
    /// RUSTFLOW_MONGODB_URI and RUSTFLOW_MONGODB_DATABASE.
    ///
    /// Optional positive integer timeout variables are
    /// RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS,
    /// RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS, and
    /// RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS.
    pub fn from_app_config(config: &AppConfig) -> Result<Self, MongoError> {
        let uri = required_setting(config, "RUSTFLOW_MONGODB_URI")?;
        let database = required_setting(config, "RUSTFLOW_MONGODB_DATABASE")?;
        let mut result = Self::new(uri, database)?;

        if let Some(value) = setting(config, "RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS")? {
            result = result.with_connect_timeout(parse_timeout(
                "RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS",
                &value,
            )?)?;
        }
        if let Some(value) = setting(config, "RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS")? {
            result = result.with_server_selection_timeout(parse_timeout(
                "RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS",
                &value,
            )?)?;
        }
        if let Some(value) = setting(config, "RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS")? {
            result = result.with_operation_timeout(parse_timeout(
                "RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS",
                &value,
            )?)?;
        }
        Ok(result)
    }

    /// Set the timeout for parsing connection settings and establishing sockets.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Result<Self, MongoError> {
        self.connect_timeout = validate_timeout("connect timeout", timeout)?;
        Ok(self)
    }

    /// Set the driver's timeout for selecting a suitable server.
    pub fn with_server_selection_timeout(
        mut self,
        timeout: Duration,
    ) -> Result<Self, MongoError> {
        self.server_selection_timeout = validate_timeout("server-selection timeout", timeout)?;
        Ok(self)
    }

    /// Set the maximum duration of an operation passed to run_with_timeout.
    pub fn with_operation_timeout(mut self, timeout: Duration) -> Result<Self, MongoError> {
        self.operation_timeout = validate_timeout("operation timeout", timeout)?;
        Ok(self)
    }

    /// Return the selected database name.
    pub fn database_name(&self) -> &str {
        &self.database
    }

    /// Return the connection timeout.
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// Return the server-selection timeout.
    pub fn server_selection_timeout(&self) -> Duration {
        self.server_selection_timeout
    }

    /// Return the timeout used by run_with_timeout.
    pub fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }
}

fn setting(config: &AppConfig, name: &str) -> Result<Option<String>, MongoError> {
    config
        .setting(name)
        .map_err(|_| MongoError::InvalidConfiguration("could not resolve MongoDB settings"))
}

fn required_setting(config: &AppConfig, name: &str) -> Result<String, MongoError> {
    setting(config, name)?.filter(|value| !value.trim().is_empty()).ok_or(
        MongoError::InvalidConfiguration(match name {
            "RUSTFLOW_MONGODB_URI" => "RUSTFLOW_MONGODB_URI must be configured",
            "RUSTFLOW_MONGODB_DATABASE" => "RUSTFLOW_MONGODB_DATABASE must be configured",
            _ => "required MongoDB setting is missing",
        }),
    )
}

fn parse_timeout(name: &'static str, value: &str) -> Result<Duration, MongoError> {
    let seconds = value.parse::<u64>().map_err(|_| {
        MongoError::InvalidConfiguration(match name {
            "RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS" => {
                "RUSTFLOW_MONGODB_CONNECT_TIMEOUT_SECS must be a positive integer"
            }
            "RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS" => {
                "RUSTFLOW_MONGODB_SERVER_SELECTION_TIMEOUT_SECS must be a positive integer"
            }
            "RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS" => {
                "RUSTFLOW_MONGODB_OPERATION_TIMEOUT_SECS must be a positive integer"
            }
            _ => "MongoDB timeout must be a positive integer",
        })
    })?;
    validate_timeout(name, Duration::from_secs(seconds))
}

fn validate_timeout(name: &'static str, timeout: Duration) -> Result<Duration, MongoError> {
    if timeout.is_zero() {
        return Err(MongoError::InvalidConfiguration(name));
    }
    Ok(timeout)
}

fn validate_database_name(name: &str) -> Result<(), MongoError> {
    if name.trim().is_empty()
        || name.len() > 63
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '.' | ' ' | '"' | '$' | '*' | '<' | '>' | ':' | '|' | '?'
                )
        })
    {
        return Err(MongoError::InvalidConfiguration(
            "MongoDB database name is empty or contains unsupported characters",
        ));
    }
    Ok(())
}

/// A declared MongoDB index used by the explicit document bootstrap contract.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentIndex {
    name: String,
    keys: Document,
    unique: bool,
}

impl DocumentIndex {
    /// Describe an index using the native MongoDB key document and uniqueness option.
    pub fn new(
        name: impl Into<String>,
        keys: Document,
        unique: bool,
    ) -> Result<Self, MongoError> {
        let name = name.into();
        if name.trim().is_empty() || name.chars().any(char::is_control) || keys.is_empty() {
            return Err(MongoError::InvalidConfiguration(
                "index name and key document must be non-empty and valid",
            ));
        }
        Ok(Self { name, keys, unique })
    }

    /// Return the index name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return a reference to the native MongoDB index keys.
    pub fn keys(&self) -> &Document {
        &self.keys
    }

    /// Whether MongoDB should enforce uniqueness for this index.
    pub fn is_unique(&self) -> bool {
        self.unique
    }
}

/// An explicit, repeatable declaration of a collection and its indexes.
///
/// Bootstrap creates missing collections and declared indexes, verifies their
/// names, and never drops existing resources. It does not manage schema migrations.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentBootstrap {
    collection: String,
    indexes: Vec<DocumentIndex>,
}

impl DocumentBootstrap {
    /// Declare a collection to prepare during bootstrap.
    pub fn new(collection: impl Into<String>) -> Result<Self, MongoError> {
        let collection = collection.into();
        if collection.trim().is_empty() || collection.chars().any(char::is_control) {
            return Err(MongoError::InvalidConfiguration(
                "MongoDB collection name must be non-empty and contain no control characters",
            ));
        }
        Ok(Self {
            collection,
            indexes: Vec::new(),
        })
    }

    /// Add an index declaration. Duplicate index names in one declaration are rejected.
    pub fn with_index(mut self, index: DocumentIndex) -> Result<Self, MongoError> {
        if self.indexes.iter().any(|existing| existing.name == index.name) {
            return Err(MongoError::InvalidConfiguration(
                "MongoDB bootstrap contains duplicate index names",
            ));
        }
        self.indexes.push(index);
        Ok(self)
    }

    /// Return the declared collection name.
    pub fn collection_name(&self) -> &str {
        &self.collection
    }

    /// Return the declared indexes.
    pub fn indexes(&self) -> &[DocumentIndex] {
        &self.indexes
    }
}

/// A cloneable MongoDB database handle backed by the official driver's client pool.
///
/// This document adapter is independent from SQLx's AnyPool. Use the driver's native
/// collection API and pass its action futures to run_with_timeout for bounded work.
#[derive(Clone)]
pub struct MongoDatabase {
    client: Client,
    database: Database,
    database_name: String,
    operation_timeout: Duration,
}

impl fmt::Debug for MongoDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MongoDatabase")
            .field("database_name", &self.database_name)
            .field("operation_timeout", &self.operation_timeout)
            .finish()
    }
}

impl MongoDatabase {
    /// Parse the URI, configure driver timeouts, create the client and verify connectivity.
    pub async fn connect(config: &MongoConfig) -> Result<Self, MongoError> {
        let parsed_options =
            tokio::time::timeout(config.connect_timeout, ClientOptions::parse(&config.uri))
                .await
                .map_err(|_| MongoError::Timeout)?
                .map_err(MongoError::InvalidUri)?;
        let mut options = parsed_options;
        options.connect_timeout = Some(config.connect_timeout);
        options.server_selection_timeout = Some(config.server_selection_timeout);
        options.default_database = Some(config.database.clone());

        let client = Client::with_options(options).map_err(MongoError::InvalidUri)?;
        let database = client.database(&config.database);
        let result = Self {
            client,
            database,
            database_name: config.database.clone(),
            operation_timeout: config.operation_timeout,
        };
        if let Err(error) = result.ping().await {
            return Err(match error {
                MongoError::Operation(error) => MongoError::Connect(error),
                other => other,
            });
        }
        Ok(result)
    }

    /// Return the configured database name.
    pub fn database_name(&self) -> &str {
        &self.database_name
    }

    /// Borrow the driver client for application-specific driver features.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Borrow the selected MongoDB database.
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Get a native driver collection handle for BSON-compatible type T.
    pub fn collection<T: Send + Sync>(&self, name: &str) -> Collection<T> {
        self.database.collection(name)
    }

    /// Execute a MongoDB driver action with RustFlow's configured operation timeout.
    ///
    /// The returned driver error is retained as the error source, but its internal details
    /// are not included in the adapter's Display or Debug output.
    pub async fn run_with_timeout<T, F>(&self, operation: F) -> Result<T, MongoError>
    where
        F: Future<Output = Result<T, crate::mongodb_driver::error::Error>>,
    {
        tokio::time::timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| MongoError::Timeout)?
            .map_err(MongoError::Operation)
    }

    /// Check connectivity with MongoDB's ping command.
    pub async fn ping(&self) -> Result<(), MongoError> {
        self.run_with_timeout(self.database.run_command(doc! { "ping": 1 }))
            .await?;
        Ok(())
    }

    /// Ensure a collection and declared indexes exist, then verify index names.
    ///
    /// Re-running bootstrap is intended to be idempotent for unchanged declarations.
    /// Existing collections are preserved; this operation never drops collections or
    /// indexes and it does not promise schema migration or index replacement semantics.
    pub async fn bootstrap(
        &self,
        declaration: &DocumentBootstrap,
    ) -> Result<(), MongoError> {
        let collection_names = self
            .run_with_timeout(self.database.list_collection_names())
            .await
            .map_err(MongoError::as_bootstrap)?;
        if !collection_names
            .iter()
            .any(|name| name == &declaration.collection)
        {
            self.run_with_timeout(self.database.create_collection(&declaration.collection))
                .await
                .map_err(MongoError::as_bootstrap)?;
        }

        let collection = self
            .database
            .collection::<Document>(&declaration.collection);
        for declared_index in &declaration.indexes {
            let mut options = IndexOptions::default();
            options.name = Some(declared_index.name.clone());
            options.unique = Some(declared_index.unique);
            let model = IndexModel::builder()
                .keys(declared_index.keys.clone())
                .options(options)
                .build();
            self.run_with_timeout(collection.create_index(model))
                .await
                .map_err(MongoError::as_bootstrap)?;
        }

        let existing_indexes = self
            .run_with_timeout(collection.list_index_names())
            .await
            .map_err(MongoError::as_bootstrap)?;
        for declared_index in &declaration.indexes {
            if !existing_indexes.iter().any(|name| name == &declared_index.name) {
                return Err(MongoError::BootstrapVerification(
                    "MongoDB did not report a declared index after bootstrap",
                ));
            }
        }
        Ok(())
    }
}

/// Serialize a Rust value into a native BSON document.
pub fn to_bson_document<T: Serialize + ?Sized>(value: &T) -> Result<Document, MongoError> {
    bson::to_document(value).map_err(MongoError::Serialization)
}

/// Deserialize a native BSON document into a Rust value.
pub fn from_bson_document<T: DeserializeOwned>(document: Document) -> Result<T, MongoError> {
    bson::from_document(document).map_err(MongoError::Deserialization)
}

/// MongoDB adapter failures with safe public messages and preserved underlying sources.
pub enum MongoError {
    /// Configuration is missing, invalid, or uses unsupported values.
    InvalidConfiguration(&'static str),
    /// The URI could not be parsed or driver options could not be constructed.
    InvalidUri(crate::mongodb_driver::error::Error),
    /// Client setup or connectivity failed.
    Connect(crate::mongodb_driver::error::Error),
    /// A native MongoDB operation failed.
    Operation(crate::mongodb_driver::error::Error),
    /// A MongoDB collection/index bootstrap operation failed.
    Bootstrap(crate::mongodb_driver::error::Error),
    /// Rust-to-BSON serialization failed.
    Serialization(crate::mongodb_driver::bson::ser::Error),
    /// BSON-to-Rust deserialization failed.
    Deserialization(crate::mongodb_driver::bson::de::Error),
    /// A configured connect or operation timeout elapsed.
    Timeout,
    /// Bootstrap completed without the expected declared index being reported.
    BootstrapVerification(&'static str),
}

impl MongoError {
    fn as_bootstrap(self) -> Self {
        match self {
            Self::Operation(error) | Self::Connect(error) | Self::InvalidUri(error) => {
                Self::Bootstrap(error)
            }
            other => other,
        }
    }

    /// Convert an adapter failure into a safe HTTP response.
    ///
    /// HTTP 409 is used only for a driver-reported duplicate-key command error.
    /// Timeouts and connection failures are reported as temporary unavailability;
    /// internal driver messages and connection credentials are never returned.
    pub fn to_http_error(&self) -> HttpError {
        match self {
            Self::Operation(error) if is_duplicate_key(error) => HttpError::new(
                StatusCode::CONFLICT,
                "A document with those values already exists",
            ),
            Self::Timeout | Self::Connect(_) => HttpError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Document database temporarily unavailable",
            ),
            _ => HttpError::internal_server_error(),
        }
    }
}

fn is_duplicate_key(error: &crate::mongodb_driver::error::Error) -> bool {
    matches!(
        error.kind.as_ref(),
        crate::mongodb_driver::error::ErrorKind::Command(command) if command.code == 11000
    )
}

impl fmt::Debug for MongoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::InvalidConfiguration(_) => "InvalidConfiguration",
            Self::InvalidUri(_) => "InvalidUri",
            Self::Connect(_) => "Connect",
            Self::Operation(_) => "Operation",
            Self::Bootstrap(_) => "Bootstrap",
            Self::Serialization(_) => "Serialization",
            Self::Deserialization(_) => "Deserialization",
            Self::Timeout => "Timeout",
            Self::BootstrapVerification(_) => "BootstrapVerification",
        };
        formatter
            .debug_struct("MongoError")
            .field("kind", &kind)
            .finish()
    }
}

impl fmt::Display for MongoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfiguration(message) | Self::BootstrapVerification(message) => {
                return write!(formatter, "{message}");
            }
            Self::InvalidUri(_) => "MongoDB connection settings are invalid",
            Self::Connect(_) => "MongoDB connectivity failed",
            Self::Operation(_) => "MongoDB operation failed",
            Self::Bootstrap(_) => "MongoDB collection/index bootstrap failed",
            Self::Serialization(_) => "Rust value could not be serialized to BSON",
            Self::Deserialization(_) => "BSON document could not be deserialized",
            Self::Timeout => "MongoDB operation timed out",
        };
        formatter.write_str(message)
    }
}

impl StdError for MongoError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::InvalidUri(error)
            | Self::Connect(error)
            | Self::Operation(error)
            | Self::Bootstrap(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Deserialization(error) => Some(error),
            Self::InvalidConfiguration(_)
            | Self::Timeout
            | Self::BootstrapVerification(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mongodb_driver::bson::oid::ObjectId;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Nested {
        label: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Example {
        #[serde(rename = "_id")]
        id: ObjectId,
        nested: Nested,
        #[serde(skip_serializing_if = "Option::is_none")]
        optional: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct WrongType {
        count: i32,
    }

    #[test]
    fn config_redacts_uri_and_rejects_invalid_values() {
        let uri = "mongodb://rustflow:secret-password@localhost:27017/test?authSource=admin";
        let config = MongoConfig::new(uri, "test").expect("valid configuration");
        let debug = format!("{config:?}");
        assert!(!debug.contains(uri));
        assert!(!debug.contains("secret-password"));
        assert!(MongoConfig::new("postgres://localhost/test", "test").is_err());
        assert!(MongoConfig::new("mongodb://localhost/test", "bad/name").is_err());
        assert!(config.clone().with_operation_timeout(Duration::ZERO).is_err());
    }

    #[test]
    fn bson_round_trip_handles_nested_fields_ids_and_absent_optionals() {
        let value = Example {
            id: ObjectId::new(),
            nested: Nested {
                label: "nested".to_owned(),
            },
            optional: None,
        };
        let document = to_bson_document(&value).expect("serialize BSON");
        assert!(!document.contains_key("optional"));
        let decoded: Example = from_bson_document(document).expect("deserialize BSON");
        assert_eq!(decoded, value);
    }

    #[test]
    fn bson_type_mismatch_is_reported() {
        let error = from_bson_document::<WrongType>(doc! { "count": "not-an-integer" })
            .expect_err("type mismatch must fail");
        assert!(matches!(error, MongoError::Deserialization(_)));
        assert!(std::error::Error::source(&error).is_some());
    }
}
