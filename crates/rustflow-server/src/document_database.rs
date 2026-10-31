//! A document-oriented MongoDB adapter kept separate from SQLx and SQL migrations.
//!
//! The low-level adapter owns the official driver client, BSON helpers and collection lifecycle.
//! The independent typed document ODM is implemented in the sibling document_odm module.

use std::{error::Error as StdError, fmt, future::{Future, IntoFuture}, time::Duration};

use crate::mongodb_driver::{
    Client, Collection, Database, IndexModel,
    bson::{self, Document, doc},
    options::{ClientOptions, IndexOptions},
};
use http::StatusCode;
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
            || !uri.starts_with("mongodb://")
            || uri.ends_with("://")
        {
            return Err(MongoError::InvalidConfiguration(
                "MongoDB URI must use mongodb:// and contain no control characters",
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
    pub fn with_server_selection_timeout(mut self, timeout: Duration) -> Result<Self, MongoError> {
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
    setting(config, name)?
        .filter(|value| !value.trim().is_empty())
        .ok_or(MongoError::InvalidConfiguration(match name {
            "RUSTFLOW_MONGODB_URI" => "RUSTFLOW_MONGODB_URI must be configured",
            "RUSTFLOW_MONGODB_DATABASE" => "RUSTFLOW_MONGODB_DATABASE must be configured",
            _ => "required MongoDB setting is missing",
        }))
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
    pub fn new(name: impl Into<String>, keys: Document, unique: bool) -> Result<Self, MongoError> {
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
/// Bootstrap creates missing collections and declared indexes, verifies declared
/// index key documents and uniqueness, and never drops existing resources. It does not manage schema migrations.
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
        if self
            .indexes
            .iter()
            .any(|existing| existing.name == index.name)
        {
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
        F: IntoFuture<Output = Result<T, crate::mongodb_driver::error::Error>>,
    {
        tokio::time::timeout(self.operation_timeout, operation.into_future())
            .await
            .map_err(|_| MongoError::Timeout)?
            .map_err(MongoError::Operation)
    }

    /// Execute an owned driver future in a detached task and time out only its JoinHandle.
    ///
    /// The driver future is polled to completion even if the caller times out, avoiding
    /// cancellation of an in-flight driver operation. A timeout does not reveal whether a
    /// write reached or was applied by the server; callers must reconcile that outcome.
    pub(crate) async fn run_owned_with_timeout<T, F>(
        &self,
        operation: F,
    ) -> Result<T, MongoError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, crate::mongodb_driver::error::Error>> + Send + 'static,
    {
        let task = tokio::spawn(operation);
        let result = tokio::time::timeout(self.operation_timeout, task)
            .await
            .map_err(|_| MongoError::Timeout)?
            .map_err(MongoError::TaskJoin)?;
        result.map_err(MongoError::Operation)
    }

    /// Check connectivity with MongoDB's ping command.
    pub async fn ping(&self) -> Result<(), MongoError> {
        self.run_with_timeout(self.database.run_command(doc! { "ping": 1 }))
            .await?;
        Ok(())
    }

    /// Ensure a collection and declared indexes exist, then verify their managed properties.
    ///
    /// Re-running bootstrap is intended to be idempotent for unchanged declarations.
    /// Existing collections are preserved; this operation never drops collections or
    /// indexes and it does not provide schema migration or index replacement semantics.
    pub async fn bootstrap(&self, declaration: &DocumentBootstrap) -> Result<(), MongoError> {
        let database = self.database.clone();
        let collection_names = self
            .run_owned_with_timeout(async move { database.list_collection_names().await })
            .await
            .map_err(MongoError::into_bootstrap)?;

        if !collection_names.iter().any(|name| name == &declaration.collection) {
            let database = self.database.clone();
            let collection_name = declaration.collection.clone();
            self.run_owned_with_timeout(async move {
                database.create_collection(&collection_name).await
            })
            .await
            .map_err(MongoError::into_bootstrap)?;
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
            let collection = collection.clone();
            self.run_owned_with_timeout(async move {
                collection.create_index(model).await
            })
            .await
            .map_err(MongoError::into_bootstrap)?;
        }

        let collection = collection.clone();
        let mut cursor = self
            .run_owned_with_timeout(async move { collection.list_indexes().await })
            .await
            .map_err(MongoError::into_bootstrap)?;

        let mut existing_indexes = Vec::new();
        loop {
            let (available, next_cursor) = self
                .run_owned_with_timeout(async move {
                    let mut cursor = cursor;
                    let available = cursor.advance().await?;
                    Ok((available, cursor))
                })
                .await
                .map_err(MongoError::into_bootstrap)?;
            cursor = next_cursor;
            if !available {
                break;
            }
            existing_indexes.push(
                cursor
                    .deserialize_current()
                    .map_err(MongoError::Deserialization)?,
            );
        }

        for declared_index in &declaration.indexes {
            let existing = existing_indexes.iter().find(|index| {
                index
                    .options
                    .as_ref()
                    .and_then(|options| options.name.as_deref())
                    == Some(declared_index.name.as_str())
            });
            let Some(existing) = existing else {
                return Err(MongoError::BootstrapVerification(
                    "MongoDB did not report a declared index after bootstrap",
                ));
            };

            let existing_unique = existing
                .options
                .as_ref()
                .and_then(|options| options.unique)
                .unwrap_or(false);
            if existing.keys != declared_index.keys || existing_unique != declared_index.unique {
                return Err(MongoError::BootstrapVerification(
                    "an existing MongoDB index has an incompatible key or uniqueness definition",
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
    /// A document model declaration or serialization contract is invalid.
    InvalidDocumentModel(&'static str),
    /// A filter, query option, or field path is invalid.
    InvalidDocumentQuery(&'static str),
    /// An update document is empty or contains conflicting operations.
    InvalidDocumentUpdate(&'static str),
    /// A spawned driver task failed before returning its operation result.
    TaskJoin(tokio::task::JoinError),
}

impl MongoError {
    fn into_bootstrap(self) -> Self {
        match self {
            Self::Operation(error) => Self::Bootstrap(error),
            Self::Connect(error) => Self::Connect(error),
            Self::InvalidUri(error) => Self::InvalidUri(error),
            other => other,
        }
    }

    /// Return whether a preserved driver error indicates an unknown transaction commit result.
    ///
    /// The caller must not assume the transaction committed or rolled back when this is true.
    pub fn is_unknown_transaction_commit_result(&self) -> bool {
        match self {
            Self::Operation(error) => error.contains_label(
                crate::mongodb_driver::error::UNKNOWN_TRANSACTION_COMMIT_RESULT,
            ),
            _ => false,
        }
    }

    /// Convert an adapter failure into a safe HTTP response.
    ///
    /// HTTP 409 is used only for driver-confirmed duplicate-key errors.
    /// Connection, server-selection, pool and timeout failures map to temporary
    /// unavailability; internal driver messages and credentials are never returned.
    pub fn to_http_error(&self) -> HttpError {
        match self {
            Self::Operation(error) | Self::Bootstrap(error) if is_duplicate_key(error) => {
                HttpError::new(StatusCode::CONFLICT, "A document with those values already exists")
            }
            Self::Operation(error) | Self::Bootstrap(error) if is_unavailable(error) => {
                HttpError::new(StatusCode::SERVICE_UNAVAILABLE, "Document database temporarily unavailable")
            }
            Self::Timeout | Self::Connect(_) => HttpError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Document database temporarily unavailable",
            ),
            Self::InvalidDocumentModel(_) | Self::InvalidDocumentQuery(_) | Self::InvalidDocumentUpdate(_) => {
                HttpError::new(StatusCode::BAD_REQUEST, "Invalid document operation")
            }
            _ => HttpError::internal_server_error(),
        }
    }
}

fn is_unavailable(error: &crate::mongodb_driver::error::Error) -> bool {
    use crate::mongodb_driver::error::{ErrorKind, WriteFailure};

    match error.kind.as_ref() {
        ErrorKind::Io(_)
        | ErrorKind::ConnectionPoolCleared { .. }
        | ErrorKind::ServerSelection { .. }
        | ErrorKind::Shutdown => true,
        ErrorKind::Command(command) => command.code == 50,
        ErrorKind::Write(WriteFailure::WriteConcernError(write_concern)) => {
            matches!(write_concern.code, 50 | 64)
        }
        ErrorKind::InsertMany(insert_many) => insert_many
            .write_concern_error
            .as_ref()
            .is_some_and(|write_concern| matches!(write_concern.code, 50 | 64)),
        _ => error.contains_label(crate::mongodb_driver::error::RETRYABLE_ERROR),
    }
}

fn is_duplicate_key(error: &crate::mongodb_driver::error::Error) -> bool {
    use crate::mongodb_driver::error::{ErrorKind, WriteFailure};

    match error.kind.as_ref() {
        ErrorKind::Command(command) => command.code == 11000,
        ErrorKind::Write(WriteFailure::WriteError(write_error)) => write_error.code == 11000,
        ErrorKind::InsertMany(insert_many) => insert_many
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|write_error| write_error.code == 11000)),
        _ => false,
    }
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
            Self::InvalidDocumentModel(_) => "InvalidDocumentModel",
            Self::InvalidDocumentQuery(_) => "InvalidDocumentQuery",
            Self::InvalidDocumentUpdate(_) => "InvalidDocumentUpdate",
            Self::TaskJoin(_) => "TaskJoin",
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
            Self::InvalidConfiguration(message)
            | Self::BootstrapVerification(message)
            | Self::InvalidDocumentModel(message)
            | Self::InvalidDocumentQuery(message)
            | Self::InvalidDocumentUpdate(message) => {
                return write!(formatter, "{message}");
            }
            Self::InvalidUri(_) => "MongoDB connection settings are invalid",
            Self::Connect(_) => "MongoDB connectivity failed",
            Self::Operation(_) => "MongoDB operation failed",
            Self::Bootstrap(_) => "MongoDB collection/index bootstrap failed",
            Self::Serialization(_) => "Rust value could not be serialized to BSON",
            Self::Deserialization(_) => "BSON document could not be deserialized",
            Self::Timeout => "MongoDB operation timed out",
            Self::TaskJoin(_) => "MongoDB operation task failed",
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
            Self::TaskJoin(error) => Some(error),
            Self::InvalidConfiguration(_)
            | Self::Timeout
            | Self::BootstrapVerification(_)
            | Self::InvalidDocumentModel(_)
            | Self::InvalidDocumentQuery(_)
            | Self::InvalidDocumentUpdate(_) => None,
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
    #[allow(dead_code)]
    struct WrongType {
        count: i32,
    }

    #[test]
    fn config_redacts_uri_and_rejects_invalid_values() {
        let uri = "mongodb://user:secret-password@localhost:27017/test?authSource=admin";
        let config = MongoConfig::new(uri, "test").expect("valid configuration");
        let debug = format!("{config:?}");
        assert!(!debug.contains(uri));
        assert!(!debug.contains("secret-password"));
        assert!(MongoConfig::new("postgres://localhost/test", "test").is_err());
        assert!(MongoConfig::new("mongodb://localhost/test", "bad/name").is_err());
        assert!(
            config
                .clone()
                .with_operation_timeout(Duration::ZERO)
                .is_err()
        );
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
