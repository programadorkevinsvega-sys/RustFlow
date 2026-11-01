//! Opt-in in-process HTTP testing support.
//!
//! Enable the testing Cargo feature on rustflow-server from a dev-dependency.
//! Requests are dispatched through the same router assembled by Application,
//! including registered handlers, middleware, body limits, request metadata and
//! configured error pages. No listener or TCP port is opened.
//!
//! In-process requests have no TCP peer address and cannot verify TLS,
//! trusted-proxy behavior, network transport, graceful shutdown or cancellation.
//! Keep TCP integration tests for those contracts.
//!
//! # Example
//!
//! ~~~rust
//! use http::StatusCode;
//! use rustflow_server::{Request, Response, testing::TestApp};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let app = TestApp::builder()
//!     .configure(|application| {
//!         application.get("/health", |_request: Request| async {
//!             Ok(Response::text("ok"))
//!         })?;
//!         Ok(())
//!     })
//!     .build()?;
//!
//! let response = app.get("/health").send().await?;
//! assert_eq!(response.status(), StatusCode::OK);
//! assert_eq!(response.text()?, "ok");
//! # Ok(())
//! # }
//! ~~~

use std::{error::Error, fmt};

use axum::body::{Body, to_bytes};
use bytes::Bytes;
use http::{
    HeaderMap, HeaderName, HeaderValue, Method, Request as HttpRequest, StatusCode,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE},
};
use serde::{Serialize, de::DeserializeOwned};
use tower::ServiceExt;

use super::{Application, BindKind, ServerConfig, ServerError};

/// Default maximum response body collected by a test request (4 MiB).
pub const DEFAULT_RESPONSE_BODY_LIMIT: usize = 4 * 1024 * 1024;

type AppConfigurator = Box<dyn FnOnce(&mut Application) -> Result<(), ServerError>>;

/// Builder for an in-process test application.
pub struct TestAppBuilder {
    application: Option<Application>,
    server_config: ServerConfig,
    configurators: Vec<AppConfigurator>,
    max_response_bytes: usize,
}

impl TestAppBuilder {
    /// Create a builder using the default server configuration.
    pub fn new() -> Self {
        Self {
            application: None,
            server_config: ServerConfig::default(),
            configurators: Vec::new(),
            max_response_bytes: DEFAULT_RESPONSE_BODY_LIMIT,
        }
    }

    /// Use an already configured application as the starting point.
    pub fn application(mut self, application: Application) -> Self {
        self.application = Some(application);
        self
    }

    /// Use explicit server limits when creating a new application.
    pub fn server_config(mut self, config: ServerConfig) -> Self {
        self.server_config = config;
        self
    }

    /// Register a configuration closure, executed in registration order.
    ///
    /// A registration error is returned by build and prevents a partially
    /// configured test application from being returned.
    pub fn configure<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut Application) -> Result<(), ServerError> + 'static,
    {
        self.configurators.push(Box::new(configure));
        self
    }

    /// Set the maximum response bytes collected by TestRequestBuilder::send.
    ///
    /// A zero limit is clamped to one byte. Request limits remain those of the
    /// configured RustFlow application.
    pub fn max_response_bytes(mut self, limit: usize) -> Self {
        self.max_response_bytes = limit.max(1);
        self
    }

    /// Build the application using RustFlow's normal router assembly.
    pub fn build(self) -> Result<TestApp, TestSupportError> {
        let mut application = self
            .application
            .unwrap_or_else(|| Application::with_config(self.server_config));

        for configure in self.configurators {
            configure(&mut application)?;
        }

        let router = application.into_router(BindKind::Plain)?;
        Ok(TestApp {
            router,
            max_response_bytes: self.max_response_bytes,
        })
    }
}

impl Default for TestAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// An in-process RustFlow application that can receive HTTP test requests.
///
/// The router is cloned for each request. State captured by route handlers
/// remains shared as it does when the application is served normally.
#[derive(Clone)]
pub struct TestApp {
    router: axum::Router,
    max_response_bytes: usize,
}

impl TestApp {
    /// Create a builder for an in-process application.
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder::new()
    }

    /// Start building a request with an arbitrary HTTP method and URI.
    pub fn request(&self, method: Method, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        TestRequestBuilder {
            app: self,
            method,
            uri: uri.into(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    /// Start a GET request.
    pub fn get(&self, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        self.request(Method::GET, uri)
    }

    /// Start a POST request.
    pub fn post(&self, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        self.request(Method::POST, uri)
    }

    /// Start a PUT request.
    pub fn put(&self, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        self.request(Method::PUT, uri)
    }

    /// Start a PATCH request.
    pub fn patch(&self, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        self.request(Method::PATCH, uri)
    }

    /// Start a DELETE request.
    pub fn delete(&self, uri: impl Into<String>) -> TestRequestBuilder<'_> {
        self.request(Method::DELETE, uri)
    }
}

/// Builder for a single in-process HTTP request.
pub struct TestRequestBuilder<'a> {
    app: &'a TestApp,
    method: Method,
    uri: String,
    headers: HeaderMap,
    body: Bytes,
}

impl TestRequestBuilder<'_> {
    /// Insert or replace a request header.
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, TestSupportError> {
        let name = HeaderName::from_bytes(name.as_bytes())?;
        let value = HeaderValue::from_str(value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Append a request header without removing earlier values.
    pub fn append_header(mut self, name: &str, value: &str) -> Result<Self, TestSupportError> {
        let name = HeaderName::from_bytes(name.as_bytes())?;
        let value = HeaderValue::from_str(value)?;
        self.headers.append(name, value);
        Ok(self)
    }

    /// Set the request Accept header.
    pub fn accept(self, value: &str) -> Result<Self, TestSupportError> {
        self.header(ACCEPT.as_str(), value)
    }

    /// Set the request Content-Type header.
    pub fn content_type(self, value: &str) -> Result<Self, TestSupportError> {
        self.header(CONTENT_TYPE.as_str(), value)
    }

    /// Add one cookie pair to the request Cookie header.
    ///
    /// Encode application-specific cookie values before passing them here.
    pub fn cookie(mut self, name: &str, value: &str) -> Result<Self, TestSupportError> {
        let mut bytes = self
            .headers
            .get(COOKIE)
            .map(|header| header.as_bytes().to_vec())
            .unwrap_or_default();

        if !bytes.is_empty() {
            bytes.extend_from_slice(b"; ");
        }
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'=');
        bytes.extend_from_slice(value.as_bytes());

        self.headers
            .insert(COOKIE, HeaderValue::from_bytes(&bytes)?);
        Ok(self)
    }

    /// Set an HTTP bearer authorization header.
    pub fn authorization_bearer(self, token: &str) -> Result<Self, TestSupportError> {
        self.header(AUTHORIZATION.as_str(), &format!("Bearer {token}"))
    }

    /// Set RustFlow's X-CSRF-Token request header.
    pub fn csrf_token(self, token: &str) -> Result<Self, TestSupportError> {
        self.header("x-csrf-token", token)
    }

    /// Append serialized query parameters to the request URI.
    pub fn query<T: Serialize + ?Sized>(mut self, value: &T) -> Result<Self, TestSupportError> {
        let encoded = serde_urlencoded::to_string(value)?;
        if !encoded.is_empty() {
            let separator = if self.uri.contains('?') {
                if self.uri.ends_with('?') || self.uri.ends_with('&') {
                    ""
                } else {
                    "&"
                }
            } else {
                "?"
            };
            self.uri.push_str(separator);
            self.uri.push_str(&encoded);
        }
        Ok(self)
    }

    /// Serialize a JSON body and set its content type when absent.
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Result<Self, TestSupportError> {
        self.body = Bytes::from(serde_json::to_vec(value)?);
        if !self.headers.contains_key(CONTENT_TYPE) {
            self.headers
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        Ok(self)
    }

    /// Serialize an application/x-www-form-urlencoded request body.
    pub fn form_urlencoded<T: Serialize + ?Sized>(
        mut self,
        value: &T,
    ) -> Result<Self, TestSupportError> {
        self.body = Bytes::from(serde_urlencoded::to_string(value)?);
        if !self.headers.contains_key(CONTENT_TYPE) {
            self.headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
        }
        Ok(self)
    }

    /// Replace the request body with raw bytes.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Dispatch the request through RustFlow's configured router.
    pub async fn send(self) -> Result<TestResponse, TestSupportError> {
        let mut request = HttpRequest::builder()
            .method(self.method)
            .uri(self.uri)
            .body(Body::from(self.body))?;
        *request.headers_mut() = self.headers;

        let response = self
            .app
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap_or_else(|never| match never {});
        let (parts, body) = response.into_parts();
        let body = to_bytes(body, self.app.max_response_bytes).await?;

        Ok(TestResponse {
            status: parts.status,
            headers: parts.headers,
            body,
        })
    }
}

/// Buffered HTTP response returned by an in-process test request.
#[derive(Debug, Clone)]
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    /// Return the HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Return all response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Return one response header by name.
    pub fn header(&self, name: &str) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    /// Return all separate Set-Cookie response headers.
    pub fn set_cookie_headers(&self) -> impl Iterator<Item = &HeaderValue> {
        self.headers.get_all(SET_COOKIE).iter()
    }

    /// Return the redirect target, if present.
    pub fn location(&self) -> Option<&HeaderValue> {
        self.headers.get(LOCATION)
    }

    /// Return the response body as bytes.
    pub fn body(&self) -> &[u8] {
        self.body.as_ref()
    }

    /// Decode the response body as UTF-8 text.
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(self.body())
    }

    /// Deserialize the response body as JSON.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(self.body())
    }
}

/// Errors returned while configuring an in-process test application or request.
#[derive(Debug)]
pub enum TestSupportError {
    /// Route, middleware, or router configuration failed.
    Server(ServerError),
    /// The HTTP request could not be constructed.
    Request(http::Error),
    /// A request header name was invalid.
    HeaderName(http::header::InvalidHeaderName),
    /// A request header value was invalid.
    HeaderValue(http::header::InvalidHeaderValue),
    /// JSON request serialization failed.
    Json(serde_json::Error),
    /// URL-encoded request serialization failed.
    Form(serde_urlencoded::ser::Error),
    /// The response body failed or exceeded the configured capture limit.
    ResponseBody(axum::Error),
}

impl fmt::Display for TestSupportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server(error) => {
                write!(formatter, "test application configuration failed: {error}")
            }
            Self::Request(error) => {
                write!(formatter, "test HTTP request could not be built: {error}")
            }
            Self::HeaderName(error) => {
                write!(formatter, "invalid test request header name: {error}")
            }
            Self::HeaderValue(error) => {
                write!(formatter, "invalid test request header value: {error}")
            }
            Self::Json(error) => write!(formatter, "test JSON serialization failed: {error}"),
            Self::Form(error) => write!(formatter, "test form serialization failed: {error}"),
            Self::ResponseBody(error) => write!(
                formatter,
                "test response body could not be collected: {error}"
            ),
        }
    }
}

impl Error for TestSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Server(error) => Some(error),
            Self::Request(error) => Some(error),
            Self::HeaderName(error) => Some(error),
            Self::HeaderValue(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Form(error) => Some(error),
            Self::ResponseBody(error) => Some(error),
        }
    }
}

impl From<ServerError> for TestSupportError {
    fn from(error: ServerError) -> Self {
        Self::Server(error)
    }
}
impl From<http::Error> for TestSupportError {
    fn from(error: http::Error) -> Self {
        Self::Request(error)
    }
}
impl From<http::header::InvalidHeaderName> for TestSupportError {
    fn from(error: http::header::InvalidHeaderName) -> Self {
        Self::HeaderName(error)
    }
}
impl From<http::header::InvalidHeaderValue> for TestSupportError {
    fn from(error: http::header::InvalidHeaderValue) -> Self {
        Self::HeaderValue(error)
    }
}
impl From<serde_json::Error> for TestSupportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl From<serde_urlencoded::ser::Error> for TestSupportError {
    fn from(error: serde_urlencoded::ser::Error) -> Self {
        Self::Form(error)
    }
}
impl From<axum::Error> for TestSupportError {
    fn from(error: axum::Error) -> Self {
        Self::ResponseBody(error)
    }
}
