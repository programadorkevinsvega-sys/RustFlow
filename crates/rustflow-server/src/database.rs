//! SQLx-backed database connections, transactions, error mapping, and migrations.
//!
//! RustFlow deliberately exposes SQLx instead of introducing an ORM. All application
//! queries should be parameterized. SQLite, PostgreSQL, MySQL, and MariaDB use the
//! runtime-generic AnyPool; MariaDB is reached through SQLx's MySQL protocol driver.

use std::{error::Error, fmt, path::Path, str::FromStr, sync::Once, time::Duration};

use http::StatusCode;
use sqlx::{
    Any, AnyConnection, AnyPool, Transaction,
    any::{AnyPoolOptions, install_default_drivers},
    migrate::Migrator,
};

use crate::{AppConfig, AppEnvironment, HttpError};

static INSTALL_DRIVERS: Once = Once::new();

/// Database backend selected by a validated connection URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    /// SQLite file or in-memory database.
    Sqlite,
    /// PostgreSQL database.
    Postgres,
    /// MySQL protocol driver, also used for compatible MariaDB servers.
    MySql,
}

/// Configuration for a SQLx connection pool.
///
/// The connection URL is intentionally omitted from Debug output because it may contain
/// credentials. Use from_app_config to respect RustFlow's dotenv precedence in
/// development/test and its process-environment-only policy in production.
#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    url: String,
    backend: DatabaseBackend,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout: Duration,
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("backend", &self.backend)
            .field("max_connections", &self.max_connections)
            .field("min_connections", &self.min_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .finish()
    }
}

impl DatabaseConfig {
    /// Validate a supported SQLx URL and choose conservative pool defaults.
    pub fn new(url: impl Into<String>) -> Result<Self, DatabaseError> {
        let url = url.into();
        if url.is_empty() || url.chars().any(char::is_control) {
            return Err(DatabaseError::InvalidConfiguration(
                "database URL is empty or contains control characters",
            ));
        }

        let backend = if url.starts_with("sqlite:") {
            DatabaseBackend::Sqlite
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            DatabaseBackend::Postgres
        } else if url.starts_with("mysql://") {
            // Parse the URL before accepting it so malformed MySQL/MariaDB settings fail
            // as configuration errors instead of being deferred to pool initialization.
            sqlx::mysql::MySqlConnectOptions::from_str(&url).map_err(|_| {
                DatabaseError::InvalidConfiguration("invalid MySQL/MariaDB connection URL")
            })?;
            DatabaseBackend::MySql
        } else {
            return Err(DatabaseError::InvalidConfiguration(
                "supported database URL schemes are sqlite:, postgres://, postgresql://, and mysql:// (MySQL/MariaDB)",
            ));
        };

        Ok(Self {
            url,
            backend,
            max_connections: 10,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(10),
        })
    }

    /// Load the database URL and pool limits using the resolved application profile.
    ///
    /// Development/test defaults to a local SQLite file. Production requires
    /// RUSTFLOW_DATABASE_URL in the process environment; dotenv files are ignored.
    /// Optional settings are RUSTFLOW_DATABASE_MAX_CONNECTIONS,
    /// RUSTFLOW_DATABASE_MIN_CONNECTIONS, and RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS.
    pub fn from_app_config(config: &AppConfig) -> Result<Self, DatabaseError> {
        let configured_url = config.setting("RUSTFLOW_DATABASE_URL").map_err(|_| {
            DatabaseError::InvalidConfiguration("could not resolve RUSTFLOW_DATABASE_URL")
        })?;

        let url = match configured_url {
            Some(url) if !url.is_empty() => url,
            _ if config.environment != AppEnvironment::Production => {
                "sqlite://rustflow.db?mode=rwc".to_owned()
            }
            _ => {
                return Err(DatabaseError::InvalidConfiguration(
                    "RUSTFLOW_DATABASE_URL must be set in the process environment for production",
                ));
            }
        };

        let mut database = Self::new(url)?;
        if let Some(value) = config
            .setting("RUSTFLOW_DATABASE_MAX_CONNECTIONS")
            .map_err(|_| {
                DatabaseError::InvalidConfiguration(
                    "could not resolve RUSTFLOW_DATABASE_MAX_CONNECTIONS",
                )
            })?
        {
            let parsed = parse_positive_u32("RUSTFLOW_DATABASE_MAX_CONNECTIONS", &value)?;
            database = database.with_max_connections(parsed)?;
        }
        if let Some(value) = config
            .setting("RUSTFLOW_DATABASE_MIN_CONNECTIONS")
            .map_err(|_| {
                DatabaseError::InvalidConfiguration(
                    "could not resolve RUSTFLOW_DATABASE_MIN_CONNECTIONS",
                )
            })?
        {
            let parsed = value.parse::<u32>().map_err(|_| {
                DatabaseError::InvalidConfiguration(
                    "RUSTFLOW_DATABASE_MIN_CONNECTIONS must be a non-negative integer",
                )
            })?;
            database = database.with_min_connections(parsed)?;
        }
        if let Some(value) = config
            .setting("RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS")
            .map_err(|_| {
                DatabaseError::InvalidConfiguration(
                    "could not resolve RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS",
                )
            })?
        {
            let parsed = parse_positive_u32("RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS", &value)?;
            database = database.with_acquire_timeout(Duration::from_secs(u64::from(parsed)))?;
        }
        Ok(database)
    }

    /// Select the validated backend.
    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    /// Configure the maximum number of open connections.
    pub fn with_max_connections(mut self, maximum: u32) -> Result<Self, DatabaseError> {
        if maximum == 0 || maximum < self.min_connections {
            return Err(DatabaseError::InvalidConfiguration(
                "maximum pool connections must be positive and greater than or equal to minimum",
            ));
        }
        self.max_connections = maximum;
        Ok(self)
    }

    /// Configure the minimum number of retained connections; zero is allowed.
    pub fn with_min_connections(mut self, minimum: u32) -> Result<Self, DatabaseError> {
        if minimum > self.max_connections {
            return Err(DatabaseError::InvalidConfiguration(
                "minimum pool connections cannot exceed maximum",
            ));
        }
        self.min_connections = minimum;
        Ok(self)
    }

    /// Configure how long a request waits to acquire a pool connection.
    pub fn with_acquire_timeout(mut self, timeout: Duration) -> Result<Self, DatabaseError> {
        if timeout.is_zero() {
            return Err(DatabaseError::InvalidConfiguration(
                "database acquire timeout must be greater than zero",
            ));
        }
        self.acquire_timeout = timeout;
        Ok(self)
    }

    /// Return the maximum pool size.
    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// Return the minimum pool size.
    pub fn min_connections(&self) -> u32 {
        self.min_connections
    }

    /// Return the connection-acquire timeout.
    pub fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }
}

fn validate_migration_dialect(
    backend: DatabaseBackend,
    directory: &Path,
) -> Result<(), DatabaseError> {
    let marker_path = directory.join(".rustflow-dialect");
    let dialect = match backend {
        DatabaseBackend::Sqlite => "sqlite",
        DatabaseBackend::Postgres => "postgres",
        DatabaseBackend::MySql => "mysql",
    };

    match std::fs::read_to_string(&marker_path) {
        Ok(value) if value.trim() == dialect => Ok(()),
        Ok(_) => Err(DatabaseError::InvalidConfiguration(
            "migration directory dialect marker does not match the configured database backend",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if backend == DatabaseBackend::MySql {
                Err(DatabaseError::InvalidConfiguration(
                    "MySQL/MariaDB migration directories require .rustflow-dialect containing mysql",
                ))
            } else {
                // Existing SQLite/PostgreSQL migration directories remain compatible.
                Ok(())
            }
        }
        Err(_) => Err(DatabaseError::InvalidConfiguration(
            "could not read migration dialect marker",
        )),
    }
}

fn parse_positive_u32(name: &'static str, value: &str) -> Result<u32, DatabaseError> {
    let parsed = value.parse::<u32>().map_err(|_| {
        DatabaseError::InvalidConfiguration(match name {
            "RUSTFLOW_DATABASE_MAX_CONNECTIONS" => {
                "RUSTFLOW_DATABASE_MAX_CONNECTIONS must be a positive integer"
            }
            "RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS" => {
                "RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS must be a positive integer"
            }
            _ => "database setting must be a positive integer",
        })
    })?;
    if parsed == 0 {
        return Err(DatabaseError::InvalidConfiguration(match name {
            "RUSTFLOW_DATABASE_MAX_CONNECTIONS" => {
                "RUSTFLOW_DATABASE_MAX_CONNECTIONS must be greater than zero"
            }
            "RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS" => {
                "RUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS must be greater than zero"
            }
            _ => "database setting must be greater than zero",
        }));
    }
    Ok(parsed)
}

/// A cloneable SQLx pool for supported SQLite, PostgreSQL, and MySQL-protocol backends.
///
/// The pool is a database resource, not an ORM or model layer. Share it with request
/// handlers by using `Arc<DatabasePool>` and issue parameterized queries through `pool()`.
#[derive(Clone)]
pub struct DatabasePool {
    pool: AnyPool,
    backend: DatabaseBackend,
    max_connections: u32,
}

impl fmt::Debug for DatabasePool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabasePool")
            .field("backend", &self.backend)
            .field("max_connections", &self.max_connections)
            .field("size", &self.pool.size())
            .field("num_idle", &self.pool.num_idle())
            .finish()
    }
}

impl DatabasePool {
    /// Connect with the supplied pool and timeout configuration.
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        INSTALL_DRIVERS.call_once(install_default_drivers);
        let pool = AnyPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.acquire_timeout)
            .connect(&config.url)
            .await
            .map_err(DatabaseError::Connect)?;
        Ok(Self {
            pool,
            backend: config.backend,
            max_connections: config.max_connections,
        })
    }

    /// Access the underlying SQLx pool for parameterized queries and row decoding.
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Return which SQLx driver this pool uses.
    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    /// Return the configured maximum number of connections.
    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// Return the current number of open connections.
    pub fn size(&self) -> u32 {
        self.pool.size()
    }

    /// Begin a transaction. Call commit to persist it or rollback to undo it.
    pub async fn begin_transaction(&self) -> Result<DatabaseTransaction, DatabaseError> {
        let inner = self
            .pool
            .begin()
            .await
            .map_err(DatabaseError::Transaction)?;
        Ok(DatabaseTransaction { inner })
    }

    /// Apply pending SQLx migrations from a directory.
    ///
    /// Re-applying an already-current directory is safe: SQLx tracks versions and
    /// checksums in _sqlx_migrations, rejecting edits to applied migrations. MySQL-family
    /// directories must contain `.rustflow-dialect` with the value `mysql` so an unmarked
    /// SQLite/PostgreSQL directory is not accidentally applied to those servers.
    pub async fn run_migrations(&self, directory: impl AsRef<Path>) -> Result<(), DatabaseError> {
        let directory = directory.as_ref();
        validate_migration_dialect(self.backend, directory)?;
        let migrator = Migrator::new(directory)
            .await
            .map_err(DatabaseError::Migration)?;
        migrator
            .run(&self.pool)
            .await
            .map_err(DatabaseError::Migration)
    }

    /// Explicitly revert reversible migrations newer than target_version.
    ///
    /// This operation is never run implicitly. SQLx needs corresponding .down.sql
    /// files. In production, prefer a forward corrective migration unless the rollback
    /// procedure has been rehearsed against a backup.
    pub async fn rollback_migrations_to(
        &self,
        directory: impl AsRef<Path>,
        target_version: i64,
    ) -> Result<(), DatabaseError> {
        if target_version < 0 {
            return Err(DatabaseError::InvalidConfiguration(
                "migration rollback target cannot be negative",
            ));
        }
        let directory = directory.as_ref();
        validate_migration_dialect(self.backend, directory)?;
        let migrator = Migrator::new(directory)
            .await
            .map_err(DatabaseError::Migration)?;
        migrator
            .undo(&self.pool, target_version)
            .await
            .map_err(DatabaseError::Migration)
    }

    /// Close the pool and wait for its connections to be dropped.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// A transaction borrowed from the pool with explicit commit/rollback semantics.
pub struct DatabaseTransaction {
    inner: Transaction<'static, Any>,
}

impl DatabaseTransaction {
    /// Access the underlying connection while this transaction owns it.
    ///
    /// Use SQLx parameterized queries against this connection, then explicitly commit
    /// or roll back. Dropping an uncommitted transaction requests a rollback.
    pub fn connection(&mut self) -> &mut AnyConnection {
        &mut self.inner
    }

    /// Commit all operations in this transaction.
    pub async fn commit(self) -> Result<(), DatabaseError> {
        self.inner
            .commit()
            .await
            .map_err(DatabaseError::Transaction)
    }

    /// Roll back all operations in this transaction.
    pub async fn rollback(self) -> Result<(), DatabaseError> {
        self.inner
            .rollback()
            .await
            .map_err(DatabaseError::Transaction)
    }
}

/// Database failures mapped to generic public messages rather than leaking SQL or URLs.
#[derive(Debug)]
pub enum DatabaseError {
    /// Invalid URL, pool sizing, timeout, or migration rollback configuration.
    InvalidConfiguration(&'static str),
    /// Initial pool connection failed.
    Connect(sqlx::Error),
    /// A query or result-decoding operation failed.
    Query(sqlx::Error),
    /// A transaction could not start, commit, or roll back.
    Transaction(sqlx::Error),
    /// SQLx could not load, validate, apply, or revert migrations.
    Migration(sqlx::migrate::MigrateError),
}

impl DatabaseError {
    fn sqlx_error(&self) -> Option<&sqlx::Error> {
        match self {
            Self::Connect(error) | Self::Query(error) | Self::Transaction(error) => Some(error),
            Self::InvalidConfiguration(_) | Self::Migration(_) => None,
        }
    }

    /// Convert a database failure into a safe HTTP error response.
    ///
    /// Row-not-found maps to 404, known uniqueness constraints map to 409, pool
    /// exhaustion maps to 503, and everything else is a generic 500. Driver messages,
    /// SQL text, credentials, and connection URLs are never exposed to the caller.
    pub fn to_http_error(&self) -> HttpError {
        match self.sqlx_error() {
            Some(sqlx::Error::RowNotFound) => HttpError::not_found("Record not found"),
            Some(sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed) => HttpError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Database temporarily unavailable",
            ),
            Some(sqlx::Error::Database(error)) if is_unique_constraint_error(error.as_ref()) => {
                HttpError::new(
                    StatusCode::CONFLICT,
                    "A record with those values already exists",
                )
            }
            _ => HttpError::internal_server_error(),
        }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid database configuration: {message}")
            }
            Self::Connect(_) => formatter.write_str("database connection failed"),
            Self::Query(_) => formatter.write_str("database operation failed"),
            Self::Transaction(_) => formatter.write_str("database transaction failed"),
            Self::Migration(_) => formatter.write_str("database migration failed"),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Connect(error) | Self::Query(error) | Self::Transaction(error) => Some(error),
            Self::Migration(error) => Some(error),
            Self::InvalidConfiguration(_) => None,
        }
    }
}

fn is_unique_constraint_error(error: &dyn sqlx::error::DatabaseError) -> bool {
    let code = error.code();
    error.is_unique_violation() || matches!(code.as_deref(), Some("23505" | "1555" | "2067"))
}

impl From<sqlx::Error> for DatabaseError {
    fn from(error: sqlx::Error) -> Self {
        Self::Query(error)
    }
}

impl From<sqlx::migrate::MigrateError> for DatabaseError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DatabaseBackend, DatabaseConfig, DatabaseError, DatabasePool, validate_migration_dialect,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn sqlite_migrations() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sqlite_migrations")
    }

    #[test]
    fn config_accepts_only_supported_database_schemes_and_redacts_urls() {
        let config = DatabaseConfig::new("postgres://user:top-secret@localhost/app").unwrap();
        assert_eq!(config.backend(), DatabaseBackend::Postgres);
        assert!(!format!("{config:?}").contains("top-secret"));
        assert_eq!(
            DatabaseConfig::new("sqlite::memory:").unwrap().backend(),
            DatabaseBackend::Sqlite
        );
        assert_eq!(
            DatabaseConfig::new("mysql://user:password@localhost/app")
                .unwrap()
                .backend(),
            DatabaseBackend::MySql
        );
        assert!(DatabaseConfig::new("mysql://[invalid/app").is_err());
        assert!(DatabaseConfig::new("mariadb://localhost/app").is_err());
        assert!(DatabaseConfig::new("not-a-url").is_err());
    }

    #[test]
    fn pool_builder_rejects_inconsistent_bounds_and_zero_timeout() {
        assert!(
            DatabaseConfig::new("sqlite::memory:")
                .unwrap()
                .with_max_connections(0)
                .is_err()
        );
        assert!(
            DatabaseConfig::new("sqlite::memory:")
                .unwrap()
                .with_min_connections(11)
                .is_err()
        );
        assert!(
            DatabaseConfig::new("sqlite::memory:")
                .unwrap()
                .with_acquire_timeout(Duration::ZERO)
                .is_err()
        );
    }

    #[test]
    fn migration_dialect_marker_is_required_and_backend_specific() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rustflow-dialect-{}-{unique}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let marker = directory.join(".rustflow-dialect");

        // Legacy SQLite/PostgreSQL directories without markers remain compatible.
        assert!(validate_migration_dialect(DatabaseBackend::Sqlite, &directory).is_ok());
        assert!(validate_migration_dialect(DatabaseBackend::Postgres, &directory).is_ok());
        assert!(validate_migration_dialect(DatabaseBackend::MySql, &directory).is_err());

        fs::write(&marker, "sqlite\n").unwrap();
        assert!(validate_migration_dialect(DatabaseBackend::Sqlite, &directory).is_ok());
        assert!(validate_migration_dialect(DatabaseBackend::Postgres, &directory).is_err());
        assert!(validate_migration_dialect(DatabaseBackend::MySql, &directory).is_err());

        fs::write(&marker, "postgres\n").unwrap();
        assert!(validate_migration_dialect(DatabaseBackend::Postgres, &directory).is_ok());
        assert!(validate_migration_dialect(DatabaseBackend::Sqlite, &directory).is_err());
        assert!(validate_migration_dialect(DatabaseBackend::MySql, &directory).is_err());

        fs::write(&marker, "mysql\n").unwrap();
        assert!(validate_migration_dialect(DatabaseBackend::MySql, &directory).is_ok());
        assert!(validate_migration_dialect(DatabaseBackend::Sqlite, &directory).is_err());
        assert!(validate_migration_dialect(DatabaseBackend::Postgres, &directory).is_err());

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn sqlite_crud_migrations_are_repeatable_and_reversible() {
        let config = DatabaseConfig::new("sqlite::memory:")
            .unwrap()
            .with_max_connections(1)
            .unwrap();
        let database = DatabasePool::connect(&config).await.unwrap();
        let sqlite_version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(
            sqlite_version, "3.46.0",
            "support matrix expects bundled SQLite 3.46.0, received {sqlite_version}"
        );
        println!("verified bundled SQLite version: {sqlite_version}");

        let migrations = sqlite_migrations();

        database.run_migrations(&migrations).await.unwrap();
        database.run_migrations(&migrations).await.unwrap();

        sqlx::query("INSERT INTO rustflow_audit_items (name) VALUES (?)")
            .bind("Ada")
            .execute(database.pool())
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_audit_items")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);

        database
            .rollback_migrations_to(&migrations, 0)
            .await
            .unwrap();
        database.run_migrations(&migrations).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_audit_items")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn sqlite_transaction_rolls_back_parameterized_writes() {
        let config = DatabaseConfig::new("sqlite::memory:")
            .unwrap()
            .with_max_connections(1)
            .unwrap();
        let database = DatabasePool::connect(&config).await.unwrap();
        sqlx::query("CREATE TABLE transaction_items (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
            .execute(database.pool())
            .await
            .unwrap();

        let mut transaction = database.begin_transaction().await.unwrap();
        sqlx::query("INSERT INTO transaction_items (value) VALUES (?)")
            .bind("discarded")
            .execute(transaction.connection())
            .await
            .unwrap();
        transaction.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transaction_items")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn optional_postgres_service_supports_pool_and_transaction_contract() {
        let url = match std::env::var("RUSTFLOW_TEST_DATABASE_URL") {
            Ok(url) if !url.trim().is_empty() => url,
            _ if std::env::var("CI").is_ok_and(|value| value == "true") => {
                panic!("RUSTFLOW_TEST_DATABASE_URL must be set to run the PostgreSQL contract in CI");
            }
            _ => return,
        };
        let config = DatabaseConfig::new(url)
            .unwrap()
            .with_max_connections(1)
            .unwrap();
        let database = DatabasePool::connect(&config).await.unwrap();
        let server_version: String = sqlx::query_scalar("SHOW server_version")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(
            server_version, "16.15",
            "support matrix expects PostgreSQL 16.15, received {server_version}"
        );
        println!("verified PostgreSQL server version: {server_version}");

        let row = sqlx::query("SELECT CAST(1 AS BIGINT)")
            .fetch_one(database.pool())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(row.try_get::<i64, _>(0).unwrap(), 1);

        let transaction = database.begin_transaction().await.unwrap();
        transaction.rollback().await.unwrap();
    }

    #[test]
    fn invalid_database_configuration_maps_to_a_safe_http_error() {
        let error = DatabaseError::InvalidConfiguration("bad input");
        assert_eq!(
            error.to_http_error().status_code(),
            http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert!(!error.to_string().contains("secret"));
    }
}
