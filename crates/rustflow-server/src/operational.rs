//! Operational configuration and extension contracts for RustFlow applications.
//!
//! This module deliberately does not install a global tracing subscriber or bind
//! the framework to a monitoring, cache, queue, email, or object-storage vendor.

use std::{error::Error, fmt, future::Future, pin::Pin, str::FromStr, sync::Arc, time::Duration};

use crate::AppConfig;

/// Maximum number of explicitly registered readiness checks.
pub const MAX_READINESS_CHECKS: usize = 32;

pub(crate) type ReadinessFuture =
    Pin<Box<dyn Future<Output = Result<(), ReadinessCheckError>> + Send + 'static>>;
type ReadinessCallback = dyn Fn() -> ReadinessFuture + Send + Sync + 'static;

/// The supported application log verbosity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Warnings and errors.
    Warn,
    /// Informational messages, warnings, and errors.
    #[default]
    Info,
    /// Debug messages and more severe messages.
    Debug,
    /// All tracing events.
    Trace,
}

impl LogLevel {
    /// Return the corresponding tracing level.
    pub fn as_tracing_level(self) -> tracing::Level {
        match self {
            Self::Error => tracing::Level::ERROR,
            Self::Warn => tracing::Level::WARN,
            Self::Info => tracing::Level::INFO,
            Self::Debug => tracing::Level::DEBUG,
            Self::Trace => tracing::Level::TRACE,
        }
    }

    /// Return the lowercase configuration value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl FromStr for LogLevel {
    type Err = OperationalConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(OperationalConfigError::InvalidLogLevel),
        }
    }
}

/// Application-owned tracing verbosity configuration.
///
/// RustFlow emits structured tracing events but never installs a global
/// subscriber. The application binary uses this value when it constructs its
/// own subscriber, formatter, filters, and exporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TracingConfig {
    level: LogLevel,
}

impl TracingConfig {
    /// Create tracing configuration with the supplied maximum level.
    pub const fn new(level: LogLevel) -> Self {
        Self { level }
    }

    /// Resolve RUSTFLOW_LOG_LEVEL through the application's normal
    /// configuration precedence. The default is info.
    ///
    /// Supported values are error, warn, info, debug, and trace.
    /// In production only process-environment values are read, consistent with
    /// AppConfig's production policy.
    pub fn from_app_config(config: &AppConfig) -> Result<Self, OperationalConfigError> {
        let value = config
            .setting("RUSTFLOW_LOG_LEVEL")
            .map_err(|_| OperationalConfigError::CouldNotResolveLogLevel)?;
        match value {
            Some(value) => Ok(Self::new(LogLevel::from_str(&value)?)),
            None => Ok(Self::default()),
        }
    }

    /// Return the configured maximum tracing level.
    pub const fn level(self) -> LogLevel {
        self.level
    }

    /// Return the tracing level used to configure an application-owned
    /// subscriber.
    pub fn tracing_level(self) -> tracing::Level {
        self.level.as_tracing_level()
    }

    /// Return the canonical lowercase level value.
    pub const fn directive(self) -> &'static str {
        self.level.as_str()
    }
}

/// Configuration for opt-in framework-owned liveness and readiness routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEndpoints {
    liveness_path: String,
    readiness_path: String,
}

impl HealthEndpoints {
    /// Create health endpoints at the supplied absolute paths.
    ///
    /// Paths must be distinct static paths. Route parameters, wildcards,
    /// query strings, fragments, and control characters are rejected.
    pub fn new(
        liveness_path: impl Into<String>,
        readiness_path: impl Into<String>,
    ) -> Result<Self, OperationalConfigError> {
        let liveness_path = liveness_path.into();
        let readiness_path = readiness_path.into();
        validate_health_path(&liveness_path)?;
        validate_health_path(&readiness_path)?;
        if liveness_path == readiness_path {
            return Err(OperationalConfigError::DuplicateHealthPath);
        }
        Ok(Self {
            liveness_path,
            readiness_path,
        })
    }

    /// Return the liveness route path.
    pub fn liveness_path(&self) -> &str {
        &self.liveness_path
    }

    /// Return the readiness route path.
    pub fn readiness_path(&self) -> &str {
        &self.readiness_path
    }
}

impl Default for HealthEndpoints {
    fn default() -> Self {
        Self {
            liveness_path: "/health/live".to_owned(),
            readiness_path: "/health/ready".to_owned(),
        }
    }
}

fn validate_health_path(path: &str) -> Result<(), OperationalConfigError> {
    if path.is_empty()
        || path.len() > 128
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.chars().any(|character| {
            character.is_control() || matches!(character, '?' | '#' | '{' | '}' | '*' | '\\')
        })
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(OperationalConfigError::InvalidHealthPath);
    }
    Ok(())
}

/// An opaque failure from an application-defined readiness check.
///
/// The value intentionally carries no message: check diagnostics must remain
/// internal and must never be serialized in the public readiness response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadinessCheckError;

impl ReadinessCheckError {
    /// Mark a required dependency as unavailable without exposing its details.
    pub const fn unavailable() -> Self {
        Self
    }
}

impl fmt::Display for ReadinessCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("readiness check failed")
    }
}

impl Error for ReadinessCheckError {}

/// A named, time-bounded check for a dependency required before serving traffic.
#[derive(Clone)]
pub struct ReadinessCheck {
    name: String,
    timeout: Duration,
    callback: Arc<ReadinessCallback>,
}

impl fmt::Debug for ReadinessCheck {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadinessCheck")
            .field("name", &self.name)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl ReadinessCheck {
    /// Create a readiness check from an async callback.
    ///
    /// Names are stable identifiers used only for application-side registration
    /// and diagnostics. The framework does not include names or error details in
    /// public readiness responses.
    pub fn new<F, Fut>(
        name: impl Into<String>,
        timeout: Duration,
        callback: F,
    ) -> Result<Self, OperationalConfigError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ReadinessCheckError>> + Send + 'static,
    {
        let name = name.into();
        if name.is_empty()
            || name.len() > 64
            || !name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            })
        {
            return Err(OperationalConfigError::InvalidReadinessCheckName);
        }
        if timeout.is_zero() {
            return Err(OperationalConfigError::ZeroReadinessTimeout);
        }
        let callback: Arc<ReadinessCallback> =
            Arc::new(move || -> ReadinessFuture { Box::pin(callback()) });
        Ok(Self {
            name,
            timeout,
            callback,
        })
    }

    /// Return the check's stable registration name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the maximum time allowed for this check.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn start(&self) -> ReadinessFuture {
        (self.callback)()
    }
}

/// Safe category describing where an internal HTTP failure was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorReportKind {
    /// A route handler returned a server-error status.
    Handler,
    /// The middleware registry could not be read.
    Middleware,
    /// The configured HTTP request deadline expired.
    RequestTimeout,
}

impl ErrorReportKind {
    /// Return a stable, non-sensitive category label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Handler => "handler",
            Self::Middleware => "middleware",
            Self::RequestTimeout => "request_timeout",
        }
    }
}

/// Safe metadata supplied to an application's internal error reporter.
///
/// This structure intentionally excludes error messages, stack traces, request
/// headers, cookies, authorization data, query values, and request bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReport {
    request_id: String,
    method: http::Method,
    route: String,
    status: http::StatusCode,
    kind: ErrorReportKind,
}

impl ErrorReport {
    pub(crate) fn new(
        request_id: String,
        method: http::Method,
        route: String,
        status: http::StatusCode,
        kind: ErrorReportKind,
    ) -> Self {
        Self {
            request_id,
            method,
            route,
            status,
            kind,
        }
    }

    /// Return the correlation identifier assigned by the HTTP adapter.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Return the HTTP method.
    pub fn method(&self) -> &http::Method {
        &self.method
    }

    /// Return the registered route template, not user-supplied path values.
    pub fn route(&self) -> &str {
        &self.route
    }

    /// Return the HTTP status associated with the internal failure.
    pub fn status(&self) -> http::StatusCode {
        self.status
    }

    /// Return the safe category associated with the internal failure.
    pub fn kind(&self) -> ErrorReportKind {
        self.kind
    }
}

/// An error returned by an application-provided error reporter.
///
/// Deliberately opaque so integrations cannot accidentally serialize provider
/// diagnostics or secrets into framework logs or HTTP responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorReporterError;

impl fmt::Display for ErrorReporterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("error reporter failed")
    }
}

impl Error for ErrorReporterError {}

/// Boxed future returned by an application-owned error reporter.
pub type ErrorReporterFuture =
    Pin<Box<dyn Future<Output = Result<(), ErrorReporterError>> + Send + 'static>>;

/// Application-owned asynchronous sink for sanitized internal error events.
pub trait ErrorReporter: Send + Sync + 'static {
    /// Submit one report. Implementations must avoid blocking executor threads.
    fn report(&self, report: ErrorReport) -> ErrorReporterFuture;
}

impl<F, Fut> ErrorReporter for F
where
    F: Fn(ErrorReport) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), ErrorReporterError>> + Send + 'static,
{
    fn report(&self, report: ErrorReport) -> ErrorReporterFuture {
        Box::pin((self)(report))
    }
}

/// Configuration errors for operational capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalConfigError {
    /// RUSTFLOW_LOG_LEVEL is not one of the supported levels.
    InvalidLogLevel,
    /// RUSTFLOW_LOG_LEVEL could not be resolved safely.
    CouldNotResolveLogLevel,
    /// A health path is not a valid static absolute path.
    InvalidHealthPath,
    /// Liveness and readiness paths must differ.
    DuplicateHealthPath,
    /// A readiness check name is empty or contains unsupported characters.
    InvalidReadinessCheckName,
    /// Readiness check timeouts must be positive.
    ZeroReadinessTimeout,
}

impl fmt::Display for OperationalConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLogLevel => {
                "RUSTFLOW_LOG_LEVEL must be error, warn, info, debug, or trace"
            }
            Self::CouldNotResolveLogLevel => "could not resolve RUSTFLOW_LOG_LEVEL",
            Self::InvalidHealthPath => "health endpoint paths must be valid static absolute paths",
            Self::DuplicateHealthPath => "liveness and readiness paths must be different",
            Self::InvalidReadinessCheckName => {
                "readiness check names must be 1-64 ASCII letters, digits, dots, underscores, or hyphens"
            }
            Self::ZeroReadinessTimeout => "readiness check timeout must be greater than zero",
        })
    }
}

impl Error for OperationalConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_levels_parse_case_insensitively_and_default_to_info() {
        assert_eq!(LogLevel::default(), LogLevel::Info);
        assert_eq!("ERROR".parse::<LogLevel>(), Ok(LogLevel::Error));
        assert_eq!("warning".parse::<LogLevel>(), Ok(LogLevel::Warn));
        assert_eq!("debug".parse::<LogLevel>(), Ok(LogLevel::Debug));
        assert_eq!("trace".parse::<LogLevel>(), Ok(LogLevel::Trace));
        assert!("verbose".parse::<LogLevel>().is_err());
        assert_eq!(TracingConfig::new(LogLevel::Warn).directive(), "warn");
        assert_eq!(
            TracingConfig::new(LogLevel::Error).tracing_level(),
            tracing::Level::ERROR
        );
    }

    #[test]
    fn health_paths_are_static_distinct_and_absolute() {
        let endpoints = HealthEndpoints::default();
        assert_eq!(endpoints.liveness_path(), "/health/live");
        assert_eq!(endpoints.readiness_path(), "/health/ready");
        assert!(HealthEndpoints::new("/live", "/ready").is_ok());
        assert!(HealthEndpoints::new("live", "/ready").is_err());
        assert!(HealthEndpoints::new("/live/{id}", "/ready").is_err());
        assert!(HealthEndpoints::new("/live", "/live").is_err());
        assert!(HealthEndpoints::new("/../live", "/ready").is_err());
    }

    #[test]
    fn readiness_checks_require_safe_names_and_positive_timeouts() {
        assert!(
            ReadinessCheck::new("database-primary", Duration::from_secs(1), || async {
                Ok(())
            })
            .is_ok()
        );
        assert!(
            ReadinessCheck::new("database password", Duration::from_secs(1), || async {
                Ok(())
            })
            .is_err()
        );
        assert!(ReadinessCheck::new("database", Duration::ZERO, || async { Ok(()) }).is_err());
    }

    #[test]
    fn reports_expose_only_allowlisted_metadata() {
        let report = ErrorReport::new(
            "request_17".to_owned(),
            http::Method::POST,
            "/accounts/{id}".to_owned(),
            http::StatusCode::INTERNAL_SERVER_ERROR,
            ErrorReportKind::Handler,
        );

        assert_eq!(report.request_id(), "request_17");
        assert_eq!(report.method(), &http::Method::POST);
        assert_eq!(report.route(), "/accounts/{id}");
        assert_eq!(report.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(report.kind(), ErrorReportKind::Handler);
        let debug = format!("{report:?}");
        assert!(!debug.contains("Authorization"));
        assert!(!debug.contains("Cookie"));
    }
}
