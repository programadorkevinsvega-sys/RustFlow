//! Isolated SQL and MongoDB fixtures for integration tests.
//!
//! SQL fixtures use a one-connection in-memory SQLite database or a configured
//! external database explicitly marked as a test database. External SQL fixtures
//! generate unique table identifiers and never drop resources outside that namespace.
//! MongoDB fixtures use a unique database name and explicit asynchronous cleanup.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{DatabaseBackend, DatabaseConfig, DatabaseError, DatabasePool, MongoConfig, MongoDatabase, MongoError};

static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(1);

/// A collision-resistant, SQL-identifier-safe namespace for one test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestNamespace {
    prefix: String,
}

impl TestNamespace {
    /// Allocate a namespace unique to this process and allocation.
    pub fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed);
        Self {
            prefix: format!("rf_test_{:x}_{:x}_{:x}", std::process::id(), timestamp, sequence),
        }
    }

    /// Return this namespace as a MongoDB database name.
    pub fn database_name(&self) -> &str {
        &self.prefix
    }

    /// Return a unique SQL table identifier for a simple logical table name.
    ///
    /// Only ASCII letters, digits and underscores are accepted. The identifier
    /// starts with a letter and is bounded to MySQL's 64-byte table-name limit.
    pub fn table_name(&self, logical_name: &str) -> Result<String, DatabaseError> {
        if logical_name.is_empty()
            || !logical_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(DatabaseError::InvalidConfiguration(
                "test table names must contain only ASCII letters, digits, and underscores",
            ));
        }
        let table = format!("{}_{}", self.prefix, logical_name);
        if table.len() > 64 {
            return Err(DatabaseError::InvalidConfiguration(
                "test table name exceeds the backend identifier limit",
            ));
        }
        Ok(table)
    }
}

impl Default for TestNamespace {
    fn default() -> Self {
        Self::new()
    }
}

/// An isolated SQL database handle for one integration test.
///
/// sqlite_in_memory creates a fresh in-memory database with one connection, so all
/// operations through the shared SQLx/SeaORM pool see the same private database.
/// connect_test_url attaches to an explicitly named external test database and
/// generates a unique table namespace. It never creates/drops databases or schemas.
pub struct TestSqlDatabase {
    pool: DatabasePool,
    namespace: TestNamespace,
}

impl TestSqlDatabase {
    /// Create a fresh in-memory SQLite database for one test.
    pub async fn sqlite_in_memory() -> Result<Self, DatabaseError> {
        let config = DatabaseConfig::new("sqlite::memory:")?.with_max_connections(1)?;
        let pool = DatabasePool::connect(&config).await?;
        Ok(Self {
            pool,
            namespace: TestNamespace::new(),
        })
    }

    /// Connect to an external SQL test database whose database name is test-scoped.
    ///
    /// The database name must be test, start with test_, or end with _test. The
    /// fixture owns only tables with its generated namespace. It never drops the
    /// database, schema, migration ledger, or resources created by another test.
    pub async fn connect_test_url(url: impl Into<String>) -> Result<Self, DatabaseError> {
        let url = url.into();
        validate_test_database_url(&url)?;
        let config = DatabaseConfig::new(url)?.with_max_connections(4)?;
        let pool = DatabasePool::connect(&config).await?;
        Ok(Self {
            pool,
            namespace: TestNamespace::new(),
        })
    }

    /// Borrow the SQLx/SeaORM pool.
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    /// Borrow the unique identifier namespace.
    pub fn namespace(&self) -> &TestNamespace {
        &self.namespace
    }

    /// Close the SQLx pool explicitly.
    pub async fn close(self) {
        self.pool.close().await;
    }
}

fn validate_test_database_url(url: &str) -> Result<(), DatabaseError> {
    let (_, authority_and_path) = url.split_once("://").ok_or(
        DatabaseError::InvalidConfiguration("external test database URL must be absolute"),
    )?;
    let (_, path_and_query) = authority_and_path.rsplit_once('/').ok_or(
        DatabaseError::InvalidConfiguration("external test database URL must include a database name"),
    )?;
    let database = path_and_query.split(['?', '#']).next().unwrap_or_default();

    if database == "test" || database.starts_with("test_") || database.ends_with("_test") {
        Ok(())
    } else {
        Err(DatabaseError::InvalidConfiguration(
            "external test database name must be test, start with test_, or end with _test",
        ))
    }
}

/// An isolated MongoDB database for one integration test.
///
/// Every fixture gets a unique database name. Call cleanup to drop only this
/// database; cleanup is explicit and never targets a shared database.
pub struct TestMongoDatabase {
    database: MongoDatabase,
    namespace: TestNamespace,
}

impl TestMongoDatabase {
    /// Connect to MongoDB using a unique database reserved for this fixture.
    pub async fn connect(uri: impl Into<String>) -> Result<Self, MongoError> {
        let namespace = TestNamespace::new();
        let config = MongoConfig::new(uri, namespace.database_name())?;
        let database = MongoDatabase::connect(&config).await?;
        Ok(Self {
            database,
            namespace,
        })
    }

    /// Borrow the MongoDB client/database handle.
    pub fn database(&self) -> &MongoDatabase {
        &self.database
    }

    /// Borrow the unique database namespace.
    pub fn namespace(&self) -> &TestNamespace {
        &self.namespace
    }

    /// Drop only this fixture's database.
    pub async fn cleanup(self) -> Result<(), MongoError> {
        let database_handle = self.database.database().clone();
        self.database
            .run_with_timeout(database_handle.drop())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_produce_safe_unique_identifiers() {
        let first = TestNamespace::new();
        let second = TestNamespace::new();
        assert_ne!(first.database_name(), second.database_name());

        let table = first.table_name("items_2026").expect("valid table name");
        assert!(table.starts_with("rf_test_"));
        assert!(table.ends_with("_items_2026"));
        assert!(table.len() <= 64);
        assert!(first.table_name("").is_err());
        assert!(first.table_name("items; DROP TABLE users").is_err());
        assert!(first.table_name("items-name").is_err());
    }

    #[test]
    fn external_sql_fixture_rejects_unmarked_database_names() {
        assert!(validate_test_database_url("postgres://localhost/rustflow_test").is_ok());
        assert!(validate_test_database_url("mysql://localhost/test_rustflow").is_ok());
        assert!(validate_test_database_url("mysql://localhost/test").is_ok());
        assert!(validate_test_database_url("postgres://localhost/production").is_err());
        assert!(validate_test_database_url("postgres://localhost/rustflow").is_err());
    }
}
