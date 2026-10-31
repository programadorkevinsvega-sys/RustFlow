//! SQLx-backed typed pools shared with SeaORM, transactions, error mapping, and migrations.
//!
//! Each configured database owns exactly one typed SQLx pool. SeaORM is constructed from
//! a clone of that same pool handle, so ORM queries, raw SQLx access, and migrations share
//! one connection lifecycle and do not create competing pools. MariaDB uses SQLx's MySQL driver.

use std::{error::Error, fmt, path::Path, str::FromStr, sync::Arc, time::Duration};

use http::StatusCode;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction as SeaOrmTransaction, QueryResult,
    Statement, TransactionTrait, Value,
};
use sqlx::{
    MySqlPool, PgPool, SqlitePool, migrate::Migrator, mysql::MySqlPoolOptions,
    postgres::PgPoolOptions, sqlite::SqlitePoolOptions,
};

use crate::{AppConfig, AppEnvironment, HttpError, orm::EntityRepository};

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

impl DatabaseBackend {
    /// Return the stable capability contract for this driver family.
    pub const fn capabilities(self) -> DatabaseCapabilities {
        match self {
            Self::Sqlite | Self::Postgres => DatabaseCapabilities {
                upsert: true,
                insert_returning: true,
            },
            Self::MySql => DatabaseCapabilities {
                upsert: true,
                // The SeaORM MariaDB-only RETURNING feature is deliberately disabled because
                // SQLx/SeaORM expose MySQL and MariaDB through the same compile-time backend.
                insert_returning: false,
            },
        }
    }
}

/// Database features that the configured SeaORM/SQLx backend can expose.
///
/// Upsert is available through SeaORM's typed OnConflict builder on each backend, but its
/// conflict-target and duplicate-key semantics are dialect-specific. INSERT RETURNING is
/// deliberately not advertised for the shared MySQL/MariaDB backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseCapabilities {
    /// A backend-specific upsert builder is available.
    pub upsert: bool,
    /// SeaORM can return the inserted model from an INSERT statement.
    pub insert_returning: bool,
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

/// The single driver-specific SQLx pool owned by a database configuration.
#[derive(Clone)]
enum DatabaseDriverPool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
    MySql(MySqlPool),
}

/// A cloneable database resource shared by SQLx and SeaORM.
///
/// The SeaORM connection is built from a clone of the matching SQLx pool. Cloning a SQLx
/// pool handle shares the same underlying pool; it does not create another connection pool.
#[derive(Clone)]
pub struct DatabasePool {
    pool: DatabaseDriverPool,
    orm: Arc<DatabaseConnection>,
    backend: DatabaseBackend,
    max_connections: u32,
}

impl fmt::Debug for DatabasePool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabasePool")
            .field("backend", &self.backend)
            .field("max_connections", &self.max_connections)
            .field("size", &self.size())
            .finish()
    }
}

impl DatabasePool {
    /// Connect with one typed SQLx pool and attach SeaORM to that exact pool handle.
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        let pool = match config.backend {
            DatabaseBackend::Sqlite => {
                let pool = SqlitePoolOptions::new()
                    .max_connections(config.max_connections)
                    .min_connections(config.min_connections)
                    .acquire_timeout(config.acquire_timeout)
                    .connect(&config.url)
                    .await
                    .map_err(DatabaseError::Connect)?;
                DatabaseDriverPool::Sqlite(pool)
            }
            DatabaseBackend::Postgres => {
                let pool = PgPoolOptions::new()
                    .max_connections(config.max_connections)
                    .min_connections(config.min_connections)
                    .acquire_timeout(config.acquire_timeout)
                    .connect(&config.url)
                    .await
                    .map_err(DatabaseError::Connect)?;
                DatabaseDriverPool::Postgres(pool)
            }
            DatabaseBackend::MySql => {
                let pool = MySqlPoolOptions::new()
                    .max_connections(config.max_connections)
                    .min_connections(config.min_connections)
                    .acquire_timeout(config.acquire_timeout)
                    .connect(&config.url)
                    .await
                    .map_err(DatabaseError::Connect)?;
                DatabaseDriverPool::MySql(pool)
            }
        };

        let orm = match &pool {
            DatabaseDriverPool::Sqlite(pool) => {
                sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool.clone())
            }
            DatabaseDriverPool::Postgres(pool) => {
                sea_orm::SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone())
            }
            DatabaseDriverPool::MySql(pool) => {
                sea_orm::SqlxMySqlConnector::from_sqlx_mysql_pool(pool.clone())
            }
        };

        Ok(Self {
            pool,
            orm: Arc::new(orm),
            backend: config.backend,
            max_connections: config.max_connections,
        })
    }

    /// Borrow the SQLite SQLx pool when SQLite is the configured backend.
    pub fn sqlx_sqlite_pool(&self) -> Option<&SqlitePool> {
        match &self.pool {
            DatabaseDriverPool::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    /// Borrow the PostgreSQL SQLx pool when PostgreSQL is the configured backend.
    pub fn sqlx_postgres_pool(&self) -> Option<&PgPool> {
        match &self.pool {
            DatabaseDriverPool::Postgres(pool) => Some(pool),
            _ => None,
        }
    }

    /// Borrow the MySQL-protocol SQLx pool when MySQL or MariaDB is configured.
    pub fn sqlx_mysql_pool(&self) -> Option<&MySqlPool> {
        match &self.pool {
            DatabaseDriverPool::MySql(pool) => Some(pool),
            _ => None,
        }
    }

    /// Access the SeaORM connection backed by the same SQLx pool.
    pub fn orm(&self) -> &DatabaseConnection {
        self.orm.as_ref()
    }
    /// Create a typed repository bound to this database's shared SeaORM connection.
    ///
    /// The repository borrows the existing connection; it never opens another pool.
    pub fn entity<E>(&self) -> EntityRepository<'_, E, DatabaseConnection>
    where
        E: sea_orm::EntityTrait,
    {
        EntityRepository::new(self.orm.as_ref())
    }

    /// Return which SQLx driver this pool uses.
    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    /// Return the explicitly supported query capabilities for this database.
    pub fn capabilities(&self) -> DatabaseCapabilities {
        self.backend.capabilities()
    }

    /// Return the configured maximum number of connections.
    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// Return the current number of open connections.
    pub fn size(&self) -> u32 {
        match &self.pool {
            DatabaseDriverPool::Sqlite(pool) => pool.size(),
            DatabaseDriverPool::Postgres(pool) => pool.size(),
            DatabaseDriverPool::MySql(pool) => pool.size(),
        }
    }

    /// Execute parameterized SQL through SeaORM on the shared pool.
    ///
    /// Values are bound by the selected SeaORM backend; use question-mark placeholders for
    /// SQLite and MySQL/MariaDB and numbered dollar placeholders for PostgreSQL.
    pub async fn execute_raw(
        &self,
        sql: &str,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<u64, DatabaseError> {
        let statement =
            Statement::from_sql_and_values(self.orm.get_database_backend(), sql, values);
        self.orm
            .execute(statement)
            .await
            .map(|result| result.rows_affected())
            .map_err(DatabaseError::OrmQuery)
    }

    /// Run a parameterized raw query through SeaORM on the shared pool.
    pub async fn query_raw(
        &self,
        sql: &str,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<Vec<QueryResult>, DatabaseError> {
        let statement =
            Statement::from_sql_and_values(self.orm.get_database_backend(), sql, values);
        self.orm
            .query_all(statement)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Begin a transaction on the shared connection pool.
    ///
    /// Both ORM operations and parameterized raw statements executed through the returned
    /// transaction use the same database transaction.
    pub async fn begin_transaction(&self) -> Result<DatabaseTransaction, DatabaseError> {
        let inner = self
            .orm
            .begin()
            .await
            .map_err(DatabaseError::OrmTransaction)?;
        Ok(DatabaseTransaction { inner })
    }

    /// Apply pending SQLx migrations from a directory using the configured typed pool.
    ///
    /// Re-applying an already-current directory is safe: SQLx tracks versions and
    /// checksums in _sqlx_migrations, rejecting edits to applied migrations. MySQL-family
    /// directories must contain a dialect marker with the value mysql so an unmarked
    /// SQLite/PostgreSQL directory is not accidentally applied to those servers.
    pub async fn run_migrations(&self, directory: impl AsRef<Path>) -> Result<(), DatabaseError> {
        let directory = directory.as_ref();
        validate_migration_dialect(self.backend, directory)?;
        let migrator = Migrator::new(directory)
            .await
            .map_err(DatabaseError::Migration)?;
        let result = match &self.pool {
            DatabaseDriverPool::Sqlite(pool) => migrator.run(pool).await,
            DatabaseDriverPool::Postgres(pool) => migrator.run(pool).await,
            DatabaseDriverPool::MySql(pool) => migrator.run(pool).await,
        };
        result.map_err(DatabaseError::Migration)
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
        let result = match &self.pool {
            DatabaseDriverPool::Sqlite(pool) => migrator.undo(pool, target_version).await,
            DatabaseDriverPool::Postgres(pool) => migrator.undo(pool, target_version).await,
            DatabaseDriverPool::MySql(pool) => migrator.undo(pool, target_version).await,
        };
        result.map_err(DatabaseError::Migration)
    }

    /// Close the one underlying pool shared by SQLx and SeaORM.
    pub async fn close(&self) {
        match &self.pool {
            DatabaseDriverPool::Sqlite(pool) => pool.close().await,
            DatabaseDriverPool::Postgres(pool) => pool.close().await,
            DatabaseDriverPool::MySql(pool) => pool.close().await,
        }
    }
}

/// A SeaORM transaction opened on the same pool used by the SQLx typed accessors.
pub struct DatabaseTransaction {
    inner: SeaOrmTransaction,
}

impl DatabaseTransaction {
    /// Borrow the SeaORM transaction for ORM or ConnectionTrait operations.
    pub fn connection(&self) -> &SeaOrmTransaction {
        &self.inner
    }
    /// Create a typed repository whose operations participate in this transaction.
    pub fn entity<E>(&self) -> EntityRepository<'_, E, SeaOrmTransaction>
    where
        E: sea_orm::EntityTrait,
    {
        EntityRepository::new(&self.inner)
    }

    /// Execute parameterized raw SQL inside this transaction.
    pub async fn execute_raw(
        &self,
        sql: &str,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<u64, DatabaseError> {
        let statement =
            Statement::from_sql_and_values(self.inner.get_database_backend(), sql, values);
        self.inner
            .execute(statement)
            .await
            .map(|result| result.rows_affected())
            .map_err(DatabaseError::OrmTransaction)
    }

    /// Fetch rows from a parameterized raw query inside this transaction.
    pub async fn query_raw(
        &self,
        sql: &str,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<Vec<QueryResult>, DatabaseError> {
        let statement =
            Statement::from_sql_and_values(self.inner.get_database_backend(), sql, values);
        self.inner
            .query_all(statement)
            .await
            .map_err(DatabaseError::OrmTransaction)
    }

    /// Commit all operations in this transaction.
    pub async fn commit(self) -> Result<(), DatabaseError> {
        self.inner.commit().await.map_err(|error| {
            if commit_error_outcome_is_unknown(&error) {
                DatabaseError::CommitOutcomeUnknown(error)
            } else {
                DatabaseError::OrmTransaction(error)
            }
        })
    }

    /// Roll back all operations in this transaction.
    pub async fn rollback(self) -> Result<(), DatabaseError> {
        self.inner
            .rollback()
            .await
            .map_err(DatabaseError::OrmTransaction)
    }
}

/// Stable category for application handling of database failures.
///
/// This classification deliberately omits driver messages, SQL, constraint names and values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseErrorKind {
    InvalidConfiguration,
    InvalidModelState,
    NotFound,
    UniqueViolation,
    ForeignKeyViolation,
    CheckViolation,
    Unavailable,
    Query,
    Transaction,
    CommitOutcomeUnknown,
    Migration,
}

/// Database failures mapped to generic public messages rather than leaking SQL or URLs.
pub enum DatabaseError {
    /// Invalid URL, pool sizing, timeout, or migration rollback configuration.
    InvalidConfiguration(&'static str),
    /// An ORM operation was rejected before SQL execution because its model state was invalid.
    InvalidModelState(&'static str),
    /// Initial typed-pool connection failed.
    Connect(sqlx::Error),
    /// A direct SQLx query or result-decoding operation failed.
    Query(sqlx::Error),
    /// A SeaORM query or execution operation failed.
    OrmQuery(sea_orm::DbErr),
    /// A transaction operation received a definitive server/driver error.
    OrmTransaction(sea_orm::DbErr),
    /// The connection failed while COMMIT was in flight; the server outcome cannot be inferred.
    CommitOutcomeUnknown(sea_orm::DbErr),
    /// SQLx could not load, validate, apply, or revert migrations.
    Migration(sqlx::migrate::MigrateError),
}

impl fmt::Debug for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseError")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl DatabaseError {
    /// Return the stable public classification for this failure.
    pub fn kind(&self) -> DatabaseErrorKind {
        match self {
            Self::InvalidConfiguration(_) => DatabaseErrorKind::InvalidConfiguration,
            Self::InvalidModelState(_) => DatabaseErrorKind::InvalidModelState,
            Self::Connect(_) => DatabaseErrorKind::Unavailable,
            Self::Query(error) => classify_sqlx_error(error, DatabaseErrorKind::Query),
            Self::OrmQuery(error) => classify_orm_error(error, DatabaseErrorKind::Query),
            Self::OrmTransaction(error) => {
                classify_orm_error(error, DatabaseErrorKind::Transaction)
            }
            Self::CommitOutcomeUnknown(_) => DatabaseErrorKind::CommitOutcomeUnknown,
            Self::Migration(_) => DatabaseErrorKind::Migration,
        }
    }

    /// Convert a database failure into a safe HTTP error response.
    ///
    /// Row-not-found maps to 404, known uniqueness constraints map to 409, pool
    /// exhaustion maps to 503, and everything else is a generic 500. Driver messages,
    /// SQL text, credentials, and connection URLs are never exposed to the caller.
    pub fn to_http_error(&self) -> HttpError {
        match self.kind() {
            DatabaseErrorKind::InvalidModelState => HttpError::bad_request("Invalid model state"),
            DatabaseErrorKind::NotFound => HttpError::not_found("Record not found"),
            DatabaseErrorKind::UniqueViolation => HttpError::new(
                StatusCode::CONFLICT,
                "A record with those values already exists",
            ),
            DatabaseErrorKind::ForeignKeyViolation => HttpError::new(
                StatusCode::CONFLICT,
                "The operation conflicts with related records",
            ),
            DatabaseErrorKind::CheckViolation => HttpError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "The record does not satisfy a database constraint",
            ),
            DatabaseErrorKind::Unavailable => HttpError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Database temporarily unavailable",
            ),
            DatabaseErrorKind::CommitOutcomeUnknown => HttpError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Database commit outcome is unknown; verify the operation before retrying",
            ),
            DatabaseErrorKind::InvalidConfiguration
            | DatabaseErrorKind::Query
            | DatabaseErrorKind::Transaction
            | DatabaseErrorKind::Migration => HttpError::internal_server_error(),
        }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid database configuration: {message}")
            }
            Self::InvalidModelState(message) => {
                write!(formatter, "invalid ORM model state: {message}")
            }
            Self::Connect(_) => formatter.write_str("database connection failed"),
            Self::Query(_) | Self::OrmQuery(_) => formatter.write_str("database operation failed"),
            Self::OrmTransaction(_) => formatter.write_str("database transaction failed"),
            Self::CommitOutcomeUnknown(_) => {
                formatter.write_str("database commit outcome is unknown")
            }
            Self::Migration(_) => formatter.write_str("database migration failed"),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Connect(error) | Self::Query(error) => Some(error),
            Self::OrmQuery(error)
            | Self::OrmTransaction(error)
            | Self::CommitOutcomeUnknown(error) => Some(error),
            Self::Migration(error) => Some(error),
            Self::InvalidConfiguration(_) | Self::InvalidModelState(_) => None,
        }
    }
}

fn constraint_kind_from_code(
    code: Option<&str>,
    mysql_number: Option<u16>,
    is_unique_violation: bool,
) -> Option<DatabaseErrorKind> {
    if is_unique_violation
        || matches!(code, Some("23505" | "1555" | "2067"))
        || matches!(mysql_number, Some(1022 | 1062 | 1169 | 1586 | 1859))
    {
        return Some(DatabaseErrorKind::UniqueViolation);
    }

    if matches!(code, Some("23503" | "787"))
        || matches!(
            mysql_number,
            Some(1216 | 1217 | 1451 | 1452 | 1557 | 1761 | 1762 | 1830 | 1834)
        )
    {
        return Some(DatabaseErrorKind::ForeignKeyViolation);
    }

    if matches!(code, Some("23514" | "275"))
        || matches!(mysql_number, Some(3819))
        || (mysql_number == Some(4025) && code == Some("23000"))
    {
        return Some(DatabaseErrorKind::CheckViolation);
    }

    None
}

fn sqlx_constraint_kind(error: &dyn sqlx::error::DatabaseError) -> Option<DatabaseErrorKind> {
    // Prefer SQLx's backend-native classification. In particular, MySQL's
    // SQLSTATE 23000 is shared by several constraint types; the driver uses
    // the protocol error number to distinguish them.
    if error.is_unique_violation() {
        return Some(DatabaseErrorKind::UniqueViolation);
    }
    if error.is_foreign_key_violation() {
        return Some(DatabaseErrorKind::ForeignKeyViolation);
    }
    if error.is_check_violation() {
        return Some(DatabaseErrorKind::CheckViolation);
    }

    let mysql_number = error
        .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
        .map(sqlx::mysql::MySqlDatabaseError::number);
    constraint_kind_from_code(error.code().as_deref(), mysql_number, false)
}

fn classify_sqlx_error(error: &sqlx::Error, fallback: DatabaseErrorKind) -> DatabaseErrorKind {
    match error {
        sqlx::Error::RowNotFound => DatabaseErrorKind::NotFound,
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => DatabaseErrorKind::Unavailable,
        sqlx::Error::Database(error) => sqlx_constraint_kind(error.as_ref()).unwrap_or(fallback),
        _ => fallback,
    }
}

fn classify_orm_error(error: &sea_orm::DbErr, fallback: DatabaseErrorKind) -> DatabaseErrorKind {
    match error {
        sea_orm::DbErr::RecordNotFound(_) | sea_orm::DbErr::RecordNotUpdated => {
            DatabaseErrorKind::NotFound
        }
        sea_orm::DbErr::ConnectionAcquire(_) => DatabaseErrorKind::Unavailable,
        sea_orm::DbErr::Conn(runtime)
        | sea_orm::DbErr::Exec(runtime)
        | sea_orm::DbErr::Query(runtime) => match runtime {
            sea_orm::RuntimeErr::SqlxError(error) => classify_sqlx_error(error, fallback),
            sea_orm::RuntimeErr::Internal(_) => fallback,
        },
        sea_orm::DbErr::UpdateGetPrimaryKey | sea_orm::DbErr::AttrNotSet(_) => {
            DatabaseErrorKind::InvalidModelState
        }
        _ => fallback,
    }
}

/// SQLSTATE connection failures and server shutdown responses are ambiguous during COMMIT.
/// Ordinary server rejections (for example, constraint or serialization errors) are definitive.
fn commit_sqlstate_is_unknown(code: Option<&str>) -> bool {
    code.is_some_and(|code| code.starts_with("08") || matches!(code, "57P01" | "57P02" | "57P03"))
}

fn commit_error_outcome_is_unknown(error: &sea_orm::DbErr) -> bool {
    match error {
        sea_orm::DbErr::Conn(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
            database_error,
        )))
        | sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
            database_error,
        )))
        | sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(sqlx::Error::Database(
            database_error,
        ))) => {
            let code = database_error.code();
            commit_sqlstate_is_unknown(code.as_deref())
        }
        _ => true,
    }
}

impl From<sea_orm::DbErr> for DatabaseError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::OrmQuery(error)
    }
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
            .fetch_one(database.sqlx_sqlite_pool().expect("SQLite pool"))
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
            .execute(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_audit_items")
            .fetch_one(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .unwrap();
        assert_eq!(count, 1);

        database
            .rollback_migrations_to(&migrations, 0)
            .await
            .unwrap();
        database.run_migrations(&migrations).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_audit_items")
            .fetch_one(database.sqlx_sqlite_pool().expect("SQLite pool"))
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
            .execute(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .unwrap();

        let transaction = database.begin_transaction().await.unwrap();
        transaction
            .execute_raw(
                "INSERT INTO transaction_items (value) VALUES (?)",
                ["discarded".into()],
            )
            .await
            .unwrap();
        transaction.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transaction_items")
            .fetch_one(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn optional_postgres_service_supports_pool_and_transaction_contract() {
        let url = match std::env::var("RUSTFLOW_TEST_DATABASE_URL") {
            Ok(url) if !url.trim().is_empty() => url,
            _ if std::env::var("CI").is_ok_and(|value| value == "true") => {
                panic!(
                    "RUSTFLOW_TEST_DATABASE_URL must be set to run the PostgreSQL contract in CI"
                );
            }
            _ => return,
        };
        let config = DatabaseConfig::new(url)
            .unwrap()
            .with_max_connections(1)
            .unwrap();
        let database = DatabasePool::connect(&config).await.unwrap();
        let server_version: String = sqlx::query_scalar("SHOW server_version")
            .fetch_one(database.sqlx_postgres_pool().expect("PostgreSQL pool"))
            .await
            .unwrap();
        assert_eq!(
            server_version.split_whitespace().next(),
            Some("16.15"),
            "support matrix expects PostgreSQL 16.15, received {server_version}"
        );
        println!("verified PostgreSQL server version: {server_version}");

        let row = sqlx::query("SELECT CAST(1 AS BIGINT)")
            .fetch_one(database.sqlx_postgres_pool().expect("PostgreSQL pool"))
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(row.try_get::<i64, _>(0).unwrap(), 1);

        let transaction = database.begin_transaction().await.unwrap();
        transaction.rollback().await.unwrap();
    }

    #[test]
    fn database_error_codes_are_classified_for_all_supported_sql_drivers() {
        use super::{DatabaseErrorKind, constraint_kind_from_code};

        for code in ["23505", "1555", "2067"] {
            assert_eq!(
                constraint_kind_from_code(Some(code), None, false),
                Some(DatabaseErrorKind::UniqueViolation),
                "unique SQLSTATE/SQLite code {code}"
            );
        }
        for number in [1022, 1062, 1169, 1586, 1859] {
            assert_eq!(
                constraint_kind_from_code(Some("23000"), Some(number), false),
                Some(DatabaseErrorKind::UniqueViolation),
                "MySQL-family unique error {number}"
            );
        }
        for code in ["23503", "787"] {
            assert_eq!(
                constraint_kind_from_code(Some(code), None, false),
                Some(DatabaseErrorKind::ForeignKeyViolation),
                "foreign-key SQLSTATE/SQLite code {code}"
            );
        }
        for number in [1216, 1217, 1451, 1452, 1557, 1761, 1762, 1830, 1834] {
            assert_eq!(
                constraint_kind_from_code(Some("23000"), Some(number), false),
                Some(DatabaseErrorKind::ForeignKeyViolation),
                "MySQL-family foreign-key error {number}"
            );
        }
        for code in ["23514", "275"] {
            assert_eq!(
                constraint_kind_from_code(Some(code), None, false),
                Some(DatabaseErrorKind::CheckViolation),
                "check SQLSTATE/SQLite code {code}"
            );
        }
        assert_eq!(
            constraint_kind_from_code(Some("HY000"), Some(3819), false),
            Some(DatabaseErrorKind::CheckViolation),
            "MySQL CHECK constraint error 3819"
        );
        assert_eq!(
            constraint_kind_from_code(Some("23000"), Some(4025), false),
            Some(DatabaseErrorKind::CheckViolation),
            "MariaDB CHECK constraint error 4025"
        );
        assert_eq!(
            constraint_kind_from_code(Some("HY000"), Some(4025), false),
            None,
            "MySQL error 4025 is not MariaDB's CHECK violation"
        );
        assert_eq!(
            constraint_kind_from_code(None, None, true),
            Some(DatabaseErrorKind::UniqueViolation)
        );
        assert_eq!(constraint_kind_from_code(Some("08006"), None, false), None);
    }

    #[test]
    fn commit_connection_and_shutdown_sqlstates_are_ambiguous() {
        use super::commit_sqlstate_is_unknown;

        for code in ["08003", "08006", "08S01", "57P01", "57P02", "57P03"] {
            assert!(
                commit_sqlstate_is_unknown(Some(code)),
                "{code} must preserve an unknown COMMIT outcome"
            );
        }
        for code in ["23505", "23514", "40001", "40P01"] {
            assert!(
                !commit_sqlstate_is_unknown(Some(code)),
                "{code} is a definitive server-side transaction rejection"
            );
        }
        assert!(!commit_sqlstate_is_unknown(None));
    }

    #[test]
    fn ambiguous_commit_failures_are_not_reported_as_rolled_back() {
        use super::{DatabaseError, DatabaseErrorKind, commit_error_outcome_is_unknown};
        use sea_orm::DbErr;

        let ambiguous = DbErr::Custom("connection dropped during commit".to_owned());
        assert!(commit_error_outcome_is_unknown(&ambiguous));

        let error = DatabaseError::CommitOutcomeUnknown(ambiguous);
        assert_eq!(error.kind(), DatabaseErrorKind::CommitOutcomeUnknown);
        assert_eq!(
            error.to_http_error().status_code(),
            http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(error.to_string().contains("outcome is unknown"));
        assert!(!error.to_string().contains("connection dropped"));
    }

    #[test]
    fn orm_missing_record_updates_map_to_not_found_for_queries_and_transactions() {
        let query_error = super::DatabaseError::OrmQuery(sea_orm::DbErr::RecordNotUpdated);
        assert_eq!(query_error.kind(), super::DatabaseErrorKind::NotFound);
        assert_eq!(
            query_error.to_http_error().status_code(),
            http::StatusCode::NOT_FOUND
        );

        let transaction_error =
            super::DatabaseError::OrmTransaction(sea_orm::DbErr::RecordNotUpdated);
        assert_eq!(transaction_error.kind(), super::DatabaseErrorKind::NotFound);
        assert_eq!(
            transaction_error.to_http_error().status_code(),
            http::StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn database_error_debug_redacts_driver_details() {
        let error = DatabaseError::OrmQuery(sea_orm::DbErr::Custom(
            "postgres://admin:secret@db/internal SELECT secret_column".to_owned(),
        ));
        let debug = format!("{error:?}");

        assert!(debug.contains("Query"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("SELECT"));
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
