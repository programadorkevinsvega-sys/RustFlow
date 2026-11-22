//! Route-scoped request rate limiting using the trusted client identity.
//!
//! The first adapter is bounded, process-local memory. It is atomic within one
//! process, but its counters are neither durable nor shared across instances.

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    future::Future,
    hash::Hash,
    net::IpAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use http::{HeaderName, HeaderValue, StatusCode, header};

use crate::{HttpError, Middleware, Next, Request, Response};

const DEFAULT_MAX_KEYS: usize = 100_000;
const MAX_RATE_LIMIT_REQUESTS: u32 = 1_000_000;
const MAX_RATE_LIMIT_WINDOW_SECONDS: u64 = 86_400;
const MAX_RATE_LIMIT_SCOPE_BYTES: usize = 128;

/// Upper bound accepted by [`InMemoryRateLimitStore::with_capacity`].
pub const MAX_IN_MEMORY_RATE_LIMIT_KEYS: usize = 1_000_000;

/// Boxed future returned by an object-safe rate-limit storage port.
pub type RateLimitStoreFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RateLimitDecision, RateLimitStoreError>> + Send + 'a>>;

/// Stable, application-defined scope and trusted client address used as a bucket key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    scope: String,
    client_ip: IpAddr,
}

impl RateLimitKey {
    /// Return the validated application-defined scope.
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Return the address resolved by RustFlow's configured client-address policy.
    pub fn client_ip(&self) -> IpAddr {
        self.client_ip
    }
}

impl fmt::Debug for RateLimitKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RateLimitKey")
            .field("scope", &self.scope)
            .field("client_ip", &"[REDACTED]")
            .finish()
    }
}

/// Atomic result for one request in a fixed window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    /// Whether this request is within the configured limit.
    pub allowed: bool,
    /// Configured number of requests allowed in this window.
    pub limit: u32,
    /// Number of requests remaining after this decision.
    pub remaining: u32,
    /// Whole seconds until the fixed window resets.
    pub reset_after_seconds: u64,
}

/// Errors returned by a rate-limit storage adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitStoreError {
    /// The backing store is unavailable or its atomic operation failed.
    Unavailable,
    /// The bounded memory adapter cannot safely allocate another active key.
    CapacityExhausted,
    /// The caller supplied an invalid window or request limit.
    InvalidConfiguration,
}

impl fmt::Display for RateLimitStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "rate limit storage is unavailable",
            Self::CapacityExhausted => "rate limit storage capacity is exhausted",
            Self::InvalidConfiguration => "rate limit configuration is invalid",
        })
    }
}

impl Error for RateLimitStoreError {}

/// Storage port. Implementations must make `consume` atomic for each key and window.
pub trait RateLimitStore: Send + Sync + 'static {
    /// Count a request in the fixed epoch-aligned window, returning its limit decision.
    fn consume(
        &self,
        key: RateLimitKey,
        now_unix_seconds: u64,
        window_seconds: u64,
        max_requests: u32,
    ) -> RateLimitStoreFuture<'_>;
}

#[derive(Debug, Clone, Copy)]
struct WindowState {
    starts_at_unix: u64,
    window_seconds: u64,
    max_requests: u32,
    requests: u32,
}

/// Bounded, process-local rate-limit counters for one server process.
///
/// Counters are reset by process restart and are not shared across replicas. Use a
/// shared adapter implementing [`RateLimitStore`] when deployment-wide limits are required.
pub struct InMemoryRateLimitStore {
    entries: Mutex<HashMap<RateLimitKey, WindowState>>,
    max_keys: usize,
}

impl InMemoryRateLimitStore {
    /// Create a process-local store with a default bound of 100,000 distinct keys.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a process-local store with an explicit, bounded number of distinct keys.
    pub fn with_capacity(max_keys: usize) -> Result<Self, RateLimitConfigError> {
        if !(1..=MAX_IN_MEMORY_RATE_LIMIT_KEYS).contains(&max_keys) {
            return Err(RateLimitConfigError::InvalidCapacity);
        }
        Ok(Self {
            entries: Mutex::new(HashMap::with_capacity(max_keys.min(DEFAULT_MAX_KEYS))),
            max_keys,
        })
    }
}

impl Default for InMemoryRateLimitStore {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::with_capacity(DEFAULT_MAX_KEYS)),
            max_keys: DEFAULT_MAX_KEYS,
        }
    }
}

impl RateLimitStore for InMemoryRateLimitStore {
    fn consume(
        &self,
        key: RateLimitKey,
        now_unix_seconds: u64,
        window_seconds: u64,
        max_requests: u32,
    ) -> RateLimitStoreFuture<'_> {
        Box::pin(async move {
            if window_seconds == 0
                || window_seconds > MAX_RATE_LIMIT_WINDOW_SECONDS
                || max_requests == 0
                || max_requests > MAX_RATE_LIMIT_REQUESTS
            {
                return Err(RateLimitStoreError::InvalidConfiguration);
            }
            let window_start = now_unix_seconds - now_unix_seconds % window_seconds;
            let reset_at = window_start
                .checked_add(window_seconds)
                .ok_or(RateLimitStoreError::Unavailable)?;
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| RateLimitStoreError::Unavailable)?;

            if !entries.contains_key(&key) && entries.len() >= self.max_keys {
                entries.retain(|_, entry| {
                    entry
                        .starts_at_unix
                        .checked_add(entry.window_seconds)
                        .is_some_and(|expires_at| expires_at > now_unix_seconds)
                });
                if entries.len() >= self.max_keys {
                    return Err(RateLimitStoreError::CapacityExhausted);
                }
            }

            let state = entries.entry(key).or_insert(WindowState {
                starts_at_unix: window_start,
                window_seconds,
                max_requests,
                requests: 0,
            });
            if state.starts_at_unix != window_start
                || state.window_seconds != window_seconds
                || state.max_requests != max_requests
            {
                state.starts_at_unix = window_start;
                state.window_seconds = window_seconds;
                state.max_requests = max_requests;
                state.requests = 0;
            }

            let allowed = state.requests < max_requests;
            if allowed {
                state.requests += 1;
            }
            Ok(RateLimitDecision {
                allowed,
                limit: max_requests,
                remaining: max_requests.saturating_sub(state.requests),
                reset_after_seconds: reset_at.saturating_sub(now_unix_seconds).max(1),
            })
        })
    }
}

/// Route-scoped fixed-window limiter keyed by the trusted client IP and a stable scope.
#[derive(Clone)]
pub struct RateLimit {
    store: Arc<dyn RateLimitStore>,
    scope: String,
    max_requests: u32,
    window_seconds: u64,
}

impl RateLimit {
    /// Configure an IP-and-scope bucket with a fixed, epoch-aligned window.
    ///
    /// Attach the returned middleware to the matching route using
    /// `Application::route_middleware`. A scope should be a stable route identifier,
    /// not request data or an interpolated URL.
    pub fn new(
        store: Arc<dyn RateLimitStore>,
        scope: impl Into<String>,
        max_requests: u32,
        window: Duration,
    ) -> Result<Self, RateLimitConfigError> {
        let scope = scope.into();
        if scope.is_empty()
            || scope.len() > MAX_RATE_LIMIT_SCOPE_BYTES
            || !scope.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':')
            })
        {
            return Err(RateLimitConfigError::InvalidScope);
        }
        if !(1..=MAX_RATE_LIMIT_REQUESTS).contains(&max_requests) {
            return Err(RateLimitConfigError::InvalidRequestLimit);
        }
        let window_seconds = window.as_secs();
        if window_seconds == 0
            || window_seconds > MAX_RATE_LIMIT_WINDOW_SECONDS
            || window.subsec_nanos() != 0
        {
            return Err(RateLimitConfigError::InvalidWindow);
        }
        Ok(Self {
            store,
            scope,
            max_requests,
            window_seconds,
        })
    }

    /// Create middleware that rejects over-limit requests before the route handler.
    pub fn middleware(&self) -> Middleware {
        let limiter = self.clone();
        Middleware::new(move |request, next| {
            let limiter = limiter.clone();
            async move { limiter.check_request(request, next).await }
        })
    }

    async fn check_request(&self, request: Request, next: Next) -> Result<Response, HttpError> {
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => {
                tracing::warn!(scope = %self.scope, "system clock is before the Unix epoch");
                return Ok(service_unavailable());
            }
        };
        self.check_request_at(request, next, now).await
    }

    async fn check_request_at(
        &self,
        request: Request,
        next: Next,
        now_unix_seconds: u64,
    ) -> Result<Response, HttpError> {
        let Some(client_ip) = request.client_ip().map(normalize_client_ip) else {
            tracing::warn!(
                scope = %self.scope,
                "request rate limit could not be enforced without a client address"
            );
            return Ok(service_unavailable());
        };
        let decision = match self
            .store
            .consume(
                RateLimitKey {
                    scope: self.scope.clone(),
                    client_ip,
                },
                now_unix_seconds,
                self.window_seconds,
                self.max_requests,
            )
            .await
        {
            Ok(decision) => decision,
            Err(error) => {
                tracing::warn!(scope = %self.scope, error = %error, "request rate limit storage failed");
                return Ok(service_unavailable());
            }
        };

        let mut response = if decision.allowed {
            next.run(request).await?
        } else {
            Response::new(StatusCode::TOO_MANY_REQUESTS, "Too many requests")
        };
        response = response
            .with_header(
                HeaderName::from_static("ratelimit-limit"),
                header_value(decision.limit)?,
            )
            .with_header(
                HeaderName::from_static("ratelimit-remaining"),
                header_value(decision.remaining)?,
            )
            .with_header(
                HeaderName::from_static("ratelimit-reset"),
                header_value(decision.reset_after_seconds)?,
            )
            .with_header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if !decision.allowed {
            response = response.with_header(
                header::RETRY_AFTER,
                header_value(decision.reset_after_seconds)?,
            );
        }
        Ok(response)
    }
}

/// Invalid rate-limit scope, window, request limit, or memory-store capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitConfigError {
    InvalidScope,
    InvalidRequestLimit,
    InvalidWindow,
    InvalidCapacity,
}

impl fmt::Display for RateLimitConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidScope => "rate limit scope is invalid",
            Self::InvalidRequestLimit => "rate limit request count is invalid",
            Self::InvalidWindow => "rate limit window must be whole seconds from 1 to 86400",
            Self::InvalidCapacity => "rate limit store capacity is out of bounds",
        })
    }
}

impl Error for RateLimitConfigError {}

fn header_value(value: impl ToString) -> Result<HeaderValue, HttpError> {
    HeaderValue::from_str(&value.to_string()).map_err(|_| HttpError::internal_server_error())
}

fn normalize_client_ip(client_ip: IpAddr) -> IpAddr {
    match client_ip {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(address)),
        IpAddr::V4(address) => IpAddr::V4(address),
    }
}

fn service_unavailable() -> Response {
    Response::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Rate limit temporarily unavailable",
    )
    .with_header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use bytes::Bytes;
    use http::{HeaderMap, Method};

    fn key(scope: &str, address: [u8; 4]) -> RateLimitKey {
        RateLimitKey {
            scope: scope.to_owned(),
            client_ip: IpAddr::V4(Ipv4Addr::from(address)),
        }
    }

    fn request(client_ip: Option<IpAddr>, forwarded_for: &'static str) -> Request {
        Request {
            method: Method::GET,
            path: "/limited".to_owned(),
            query_string: None,
            path_params: HashMap::new(),
            headers: HeaderMap::from_iter([(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static(forwarded_for),
            )]),
            body: Bytes::new(),
            request_id: "rate-limit-test".to_owned(),
            peer_addr: None,
            client_ip,
            https: false,
            session: None,
        }
    }

    fn next(calls: Arc<AtomicUsize>) -> Next {
        Next {
            index: 0,
            middleware: Arc::new(Vec::new()),
            terminal: Arc::new(move |_request| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(Response::text("ok")) })
            }),
        }
    }

    #[tokio::test]
    async fn fixed_window_limits_reset_and_scopes_are_independent() {
        let store = InMemoryRateLimitStore::new();
        let first = store
            .consume(key("login", [198, 51, 100, 1]), 59, 60, 2)
            .await
            .expect("first request");
        let second = store
            .consume(key("login", [198, 51, 100, 1]), 59, 60, 2)
            .await
            .expect("second request");
        let limited = store
            .consume(key("login", [198, 51, 100, 1]), 59, 60, 2)
            .await
            .expect("limited request");
        let separate_scope = store
            .consume(key("register", [198, 51, 100, 1]), 59, 60, 2)
            .await
            .expect("independent scope");
        let next_window = store
            .consume(key("login", [198, 51, 100, 1]), 60, 60, 2)
            .await
            .expect("next window");

        assert!(first.allowed);
        assert_eq!(first.remaining, 1);
        assert_eq!(first.reset_after_seconds, 1);
        assert!(second.allowed);
        assert!(!limited.allowed);
        assert_eq!(limited.remaining, 0);
        assert!(separate_scope.allowed);
        assert!(next_window.allowed);
    }

    #[tokio::test]
    async fn concurrent_consumes_never_exceed_the_limit() {
        let store = Arc::new(InMemoryRateLimitStore::new());
        let tasks = (0..64).map(|_| {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .consume(key("login", [192, 0, 2, 4]), 100, 60, 7)
                    .await
                    .expect("atomic consume")
                    .allowed
            })
        });
        let mut allowed = 0;
        for task in tasks {
            allowed += usize::from(task.await.expect("join consume"));
        }
        assert_eq!(allowed, 7);
    }

    #[tokio::test]
    async fn exhausted_capacity_fails_closed_and_expired_keys_are_reclaimed() {
        let store = InMemoryRateLimitStore::with_capacity(1).expect("bounded capacity");
        store
            .consume(key("login", [192, 0, 2, 1]), 100, 60, 2)
            .await
            .expect("first key");
        assert_eq!(
            store
                .consume(key("login", [192, 0, 2, 2]), 100, 60, 2)
                .await,
            Err(RateLimitStoreError::CapacityExhausted)
        );
        assert!(
            store
                .consume(key("login", [192, 0, 2, 2]), 160, 60, 2)
                .await
                .expect("expired key reclaimed")
                .allowed
        );
        assert!(InMemoryRateLimitStore::with_capacity(0).is_err());
        assert!(InMemoryRateLimitStore::with_capacity(MAX_IN_MEMORY_RATE_LIMIT_KEYS + 1).is_err());
    }

    #[test]
    fn policy_rejects_unbounded_or_ambiguous_configuration() {
        let store: Arc<dyn RateLimitStore> = Arc::new(InMemoryRateLimitStore::new());
        assert!(RateLimit::new(Arc::clone(&store), "", 1, Duration::from_secs(60)).is_err());
        assert!(
            RateLimit::new(
                Arc::clone(&store),
                "login?name=",
                1,
                Duration::from_secs(60)
            )
            .is_err()
        );
        assert!(RateLimit::new(Arc::clone(&store), "login", 0, Duration::from_secs(60)).is_err());
        assert!(
            RateLimit::new(
                Arc::clone(&store),
                "login",
                MAX_RATE_LIMIT_REQUESTS + 1,
                Duration::from_secs(60)
            )
            .is_err()
        );
        assert!(RateLimit::new(Arc::clone(&store), "login", 1, Duration::ZERO).is_err());
        assert!(RateLimit::new(store, "login", 1, Duration::from_millis(1500)).is_err());
    }

    #[tokio::test]
    async fn middleware_blocks_before_handler_and_returns_retry_headers() {
        let handler_calls = Arc::new(AtomicUsize::new(0));
        let rate_limit = RateLimit::new(
            Arc::new(InMemoryRateLimitStore::new()),
            "limited",
            2,
            Duration::from_secs(60),
        )
        .expect("valid policy");
        let client_ip = "198.51.100.20".parse().ok();
        let first = rate_limit
            .check_request_at(
                request(client_ip, "203.0.113.1"),
                next(Arc::clone(&handler_calls)),
                120,
            )
            .await
            .expect("first request");
        let second = rate_limit
            .check_request_at(
                request(client_ip, "203.0.113.2"),
                next(Arc::clone(&handler_calls)),
                120,
            )
            .await
            .expect("second request");
        let third = rate_limit
            .check_request_at(
                request(client_ip, "203.0.113.3"),
                next(Arc::clone(&handler_calls)),
                120,
            )
            .await
            .expect("third request");
        assert_eq!(first.status_code(), StatusCode::OK);
        assert_eq!(second.status_code(), StatusCode::OK);
        assert_eq!(third.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(handler_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            third
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok()),
            Some("60")
        );
        assert_eq!(
            third
                .headers()
                .get("ratelimit-limit")
                .and_then(|value| value.to_str().ok()),
            Some("2")
        );
        assert_eq!(
            third
                .headers()
                .get("ratelimit-remaining")
                .and_then(|value| value.to_str().ok()),
            Some("0")
        );
        assert_eq!(
            third
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
    }

    #[tokio::test]
    async fn missing_client_identity_fails_closed_before_the_handler() {
        let handler_calls = Arc::new(AtomicUsize::new(0));
        let rate_limit = RateLimit::new(
            Arc::new(InMemoryRateLimitStore::new()),
            "limited",
            2,
            Duration::from_secs(60),
        )
        .expect("valid policy");
        let response = rate_limit
            .check_request_at(
                request(None, "203.0.113.250"),
                next(Arc::clone(&handler_calls)),
                120,
            )
            .await
            .expect("missing identity response");

        assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(handler_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn mapped_ipv4_addresses_share_the_ipv4_bucket() {
        let mapped = "::ffff:198.51.100.20"
            .parse::<IpAddr>()
            .expect("mapped address");
        assert_eq!(
            normalize_client_ip(mapped),
            "198.51.100.20".parse::<IpAddr>().expect("IPv4 address")
        );
    }
}
