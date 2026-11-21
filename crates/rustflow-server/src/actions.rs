//! Typed JSON server actions layered on the existing HTTP and security pipeline.

use std::{
    error::Error,
    fmt,
    io::{self, Write},
    sync::Arc,
};

use http::{StatusCode, header};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    AccessPolicy, CsrfProtection, HttpError, Middleware, Request, Response, ValidateInput,
};

/// Default maximum JSON request body for one server action (64 KiB).
pub const DEFAULT_ACTION_REQUEST_BYTES: usize = 64 * 1024;
/// Maximum configurable JSON request body for one server action (1 MiB).
pub const MAX_ACTION_REQUEST_BYTES: usize = 1024 * 1024;
/// Default maximum serialized JSON response for one server action (256 KiB).
pub const DEFAULT_ACTION_RESPONSE_BYTES: usize = 256 * 1024;
/// Maximum configurable serialized JSON response for one server action (4 MiB).
pub const MAX_ACTION_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Required access declaration for a server action.
#[derive(Clone)]
pub enum ActionAccess {
    /// The action is intentionally unauthenticated.
    Public,
    /// Require a server-side session identity.
    Authenticated,
    /// Require a server-side identity granted a named permission.
    Permission {
        /// Permission checked by the policy.
        permission: String,
        /// Policy that grants permissions; absence is not representable.
        policy: Arc<dyn AccessPolicy>,
    },
}

impl ActionAccess {
    /// Declare session authentication and permission policy for an action.
    pub fn permission(
        permission: impl Into<String>,
        policy: Arc<dyn AccessPolicy>,
    ) -> Result<Self, ActionConfigError> {
        let permission = permission.into();
        if permission.trim().is_empty() {
            return Err(ActionConfigError::EmptyPermission);
        }
        Ok(Self::Permission { permission, policy })
    }
}

/// Required CSRF declaration for a server action.
#[derive(Clone)]
pub enum ActionCsrf {
    /// No browser-cookie authentication or cookie-dependent state is used.
    NotRequired,
    /// Require RustFlow's signed CSRF token and same-origin request context.
    Required(CsrfProtection),
}

/// Security and payload limits for one JSON server action.
///
/// `access` and `csrf` have no defaults and must both be passed to [`new`](Self::new).
/// Session-authenticated and permission-guarded actions require CSRF because
/// they rely on browser cookies. Public actions must explicitly declare whether
/// CSRF is required when their effects depend on cookies. Use ordinary routes
/// for authentication schemes not represented by these first-contract modes.
///
/// ```compile_fail
/// use rustflow_server::{ActionAccess, ActionConfig};
///
/// let _config = ActionConfig::new(ActionAccess::Public);
/// ```
///
/// ```compile_fail
/// use rustflow_server::{ActionConfig, ActionCsrf};
///
/// let _config = ActionConfig::new(ActionCsrf::NotRequired);
/// ```
#[derive(Clone)]
pub struct ActionConfig {
    access: ActionAccess,
    csrf: ActionCsrf,
    max_request_bytes: usize,
    max_response_bytes: usize,
}

impl ActionConfig {
    /// Configure an action with mandatory access and CSRF declarations.
    ///
    /// Omitting either declaration is a Rust compile error. Cookie-authenticated
    /// access modes cannot be combined with [`ActionCsrf::NotRequired`].
    pub fn new(access: ActionAccess, csrf: ActionCsrf) -> Result<Self, ActionConfigError> {
        if let ActionAccess::Permission { permission, .. } = &access
            && permission.trim().is_empty()
        {
            return Err(ActionConfigError::EmptyPermission);
        }
        if !matches!(access, ActionAccess::Public) && matches!(csrf, ActionCsrf::NotRequired) {
            return Err(ActionConfigError::CsrfRequiredForSessionAccess);
        }
        Ok(Self {
            access,
            csrf,
            max_request_bytes: DEFAULT_ACTION_REQUEST_BYTES,
            max_response_bytes: DEFAULT_ACTION_RESPONSE_BYTES,
        })
    }

    /// Set a finite per-action request body limit.
    ///
    /// The server-wide body limit is still enforced first. Limits must be from
    /// one byte through [`MAX_ACTION_REQUEST_BYTES`].
    pub fn with_max_request_bytes(mut self, limit: usize) -> Result<Self, ActionConfigError> {
        self.max_request_bytes = bounded_limit(limit, MAX_ACTION_REQUEST_BYTES)?;
        Ok(self)
    }

    /// Set a finite per-action serialized response limit.
    ///
    /// Serialization stops as soon as the limit would be exceeded. Limits must
    /// be from one byte through [`MAX_ACTION_RESPONSE_BYTES`].
    pub fn with_max_response_bytes(mut self, limit: usize) -> Result<Self, ActionConfigError> {
        self.max_response_bytes = bounded_limit(limit, MAX_ACTION_RESPONSE_BYTES)?;
        Ok(self)
    }

    /// Return the per-action request body limit.
    pub fn max_request_bytes(&self) -> usize {
        self.max_request_bytes
    }

    /// Return the per-action serialized response limit.
    pub fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }

    pub(crate) fn middleware(&self) -> Vec<Middleware> {
        let max_request_bytes = self.max_request_bytes;
        let mut middleware = vec![Middleware::new(move |request, next| async move {
            if request.body().len() > max_request_bytes {
                return Err(HttpError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Request body is too large",
                ));
            }
            next.run(request).await
        })];
        let access = match &self.access {
            ActionAccess::Public => None,
            ActionAccess::Authenticated => Some(crate::require_authentication()),
            ActionAccess::Permission { permission, policy } => Some(crate::require_permission(
                permission.clone(),
                Some(Arc::clone(policy)),
            )),
        };
        let csrf = match &self.csrf {
            ActionCsrf::NotRequired => None,
            ActionCsrf::Required(csrf) => Some(csrf),
        };

        middleware.extend(
            access
                .into_iter()
                .chain(csrf.map(CsrfProtection::middleware)),
        );
        middleware
    }
}

/// Invalid server-action configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionConfigError {
    /// A permission name cannot be empty.
    EmptyPermission,
    /// A configured limit is zero.
    ZeroLimit,
    /// Session-based access requires CSRF protection.
    CsrfRequiredForSessionAccess,
    /// A configured limit exceeds the framework's initial bounded maximum.
    LimitTooLarge { maximum: usize },
}

impl fmt::Display for ActionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPermission => formatter.write_str("action permission must not be empty"),
            Self::ZeroLimit => formatter.write_str("action limits must be greater than zero"),
            Self::CsrfRequiredForSessionAccess => {
                formatter.write_str("session-protected actions require CSRF protection")
            }
            Self::LimitTooLarge { maximum } => {
                write!(formatter, "action limit must not exceed {maximum} bytes")
            }
        }
    }
}

impl Error for ActionConfigError {}

fn bounded_limit(limit: usize, maximum: usize) -> Result<usize, ActionConfigError> {
    if limit == 0 {
        return Err(ActionConfigError::ZeroLimit);
    }
    if limit > maximum {
        return Err(ActionConfigError::LimitTooLarge { maximum });
    }
    Ok(limit)
}

pub(crate) fn invoke<I, O, F, Fut>(
    request: Request,
    config: &ActionConfig,
    handler: F,
) -> impl std::future::Future<Output = Result<Response, HttpError>> + Send + 'static
where
    I: DeserializeOwned + ValidateInput + Send + 'static,
    O: Serialize + Send + 'static,
    F: Fn(Request, I) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<O, HttpError>> + Send + 'static,
{
    let max_response_bytes = config.max_response_bytes;
    async move {
        let mut content_types = request.headers.get_all(header::CONTENT_TYPE).iter();
        let content_type = content_types.next().and_then(|value| value.to_str().ok());
        let is_json = content_types.next().is_none()
            && content_type.is_some_and(|value| {
                value.split(';').next().is_some_and(|media_type| {
                    media_type.trim().eq_ignore_ascii_case("application/json")
                })
            });
        if !is_json {
            return Err(HttpError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Expected application/json content",
            ));
        }

        let input = request.validated_json::<I>()?;
        let output = handler(request, input).await?;
        json_response(&output, max_response_bytes)
    }
}

fn json_response<T: Serialize>(value: &T, max_bytes: usize) -> Result<Response, HttpError> {
    let mut writer = BoundedWriter::new(max_bytes);
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(HttpError::internal_server_error());
    }
    Ok(Response::new(StatusCode::OK, writer.into_bytes())
        .with_header(
            header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json; charset=utf-8"),
        )
        .with_header(
            header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        )
        .with_header(
            header::X_CONTENT_TYPE_OPTIONS,
            http::HeaderValue::from_static("nosniff"),
        ))
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(maximum.min(4096)),
            maximum,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("server action response limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionAccess, ActionConfig, ActionConfigError, ActionCsrf, DEFAULT_ACTION_REQUEST_BYTES,
        DEFAULT_ACTION_RESPONSE_BYTES, MAX_ACTION_REQUEST_BYTES, MAX_ACTION_RESPONSE_BYTES,
        bounded_limit,
    };

    #[test]
    fn action_defaults_are_bounded_and_configurable_limits_reject_invalid_values() {
        let config = ActionConfig::new(ActionAccess::Public, ActionCsrf::NotRequired)
            .expect("public action security");
        assert_eq!(config.max_request_bytes(), DEFAULT_ACTION_REQUEST_BYTES);
        assert_eq!(config.max_response_bytes(), DEFAULT_ACTION_RESPONSE_BYTES);

        assert_eq!(
            bounded_limit(0, MAX_ACTION_REQUEST_BYTES),
            Err(ActionConfigError::ZeroLimit)
        );
        assert_eq!(
            bounded_limit(MAX_ACTION_REQUEST_BYTES + 1, MAX_ACTION_REQUEST_BYTES),
            Err(ActionConfigError::LimitTooLarge {
                maximum: MAX_ACTION_REQUEST_BYTES
            })
        );
        assert_eq!(
            bounded_limit(MAX_ACTION_RESPONSE_BYTES, MAX_ACTION_RESPONSE_BYTES)
                .expect("maximum response limit"),
            MAX_ACTION_RESPONSE_BYTES
        );
        assert!(matches!(
            ActionConfig::new(ActionAccess::Authenticated, ActionCsrf::NotRequired),
            Err(ActionConfigError::CsrfRequiredForSessionAccess)
        ));
    }
}
