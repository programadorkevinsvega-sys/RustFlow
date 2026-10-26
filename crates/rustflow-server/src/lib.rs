//! RustFlow-owned HTTP request, response, routing, and server composition APIs.
//!
//! Axum, Tokio, and Tower are isolated to this adapter crate. Core reactivity and
//! the structural view model do not depend on HTTP infrastructure.

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, Request as AxumRequest},
    response::Response as AxumResponse,
    routing::{MethodFilter, MethodRouter},
};
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use http_body_util::LengthLimitError;
use serde::{Serialize, de::DeserializeOwned};
use tokio::net::TcpListener;
use tower::limit::GlobalConcurrencyLimitLayer;

use rustflow::components::ComponentRegistry;
use rustflow_view::View;

/// Future returned by a RustFlow route handler.
pub type HandlerFuture =
    Pin<Box<dyn Future<Output = Result<Response, HttpError>> + Send + 'static>>;
type SharedHandler = Arc<dyn Fn(Request) -> HandlerFuture + Send + Sync + 'static>;

type SharedMiddleware =
    Arc<dyn Fn(Request, Next) -> HandlerFuture + Send + Sync + 'static>;

/// Middleware that wraps a request/response pair and may continue or short-circuit.
///
/// Middleware executes in registration order. A middleware that calls `next.run(request)`
/// passes control to the next middleware or route handler.
#[derive(Clone)]
pub struct Middleware {
    handler: SharedMiddleware,
}

impl Middleware {
    /// Create middleware from an async closure.
    pub fn new<F, Fut>(handler: F) -> Self
    where
        F: Fn(Request, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        Self {
            handler: Arc::new(move |request, next| Box::pin(handler(request, next))),
        }
    }

    fn call(&self, request: Request, next: Next) -> HandlerFuture {
        (self.handler)(request, next)
    }
}

/// Continuation passed to middleware to run the remaining pipeline and route handler.
pub struct Next {
    index: usize,
    middleware: Arc<Vec<Middleware>>,
    terminal: SharedHandler,
}

impl Next {
    /// Run the next middleware or the terminal route handler.
    pub fn run(self, request: Request) -> HandlerFuture {
        if let Some(middleware) = self.middleware.get(self.index).cloned() {
            middleware.call(
                request,
                Self {
                    index: self.index + 1,
                    middleware: self.middleware,
                    terminal: self.terminal,
                },
            )
        } else {
            (self.terminal)(request)
        }
    }
}

/// Public description of a registered route.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteInfo {
    pub method: HttpMethod,
    pub path: String,
    pub name: Option<String>,
}


/// A validated subset of conventional HTTP methods supported by the route builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Trace,
    Connect,
}

impl HttpMethod {
    fn filter(self) -> MethodFilter {
        match self {
            Self::Get => MethodFilter::GET,
            Self::Post => MethodFilter::POST,
            Self::Put => MethodFilter::PUT,
            Self::Patch => MethodFilter::PATCH,
            Self::Delete => MethodFilter::DELETE,
            Self::Head => MethodFilter::HEAD,
            Self::Options => MethodFilter::OPTIONS,
            Self::Trace => MethodFilter::TRACE,
            Self::Connect => MethodFilter::CONNECT,
        }
    }
}

/// An incoming request after transport-level parsing and body-size enforcement.
///
/// Route handlers see normalized path/query data and buffered body bytes rather
/// than Axum request/body types. This is intentionally an adapter API, not a
/// domain-layer type.
#[derive(Debug, Clone)]
pub struct Request {
    method: Method,
    path: String,
    query_string: Option<String>,
    path_params: HashMap<String, String>,
    headers: HeaderMap,
    body: Bytes,
}

impl Request {
    fn new(
        method: Method,
        uri: Uri,
        path_params: HashMap<String, String>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Self {
        Self {
            method,
            path: uri.path().to_owned(),
            query_string: uri.query().map(str::to_owned),
            path_params,
            headers,
            body,
        }
    }

    /// Return the HTTP method.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Return the decoded URI path without the query string.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Return the raw query string, without the leading question mark.
    pub fn query_string(&self) -> Option<&str> {
        self.query_string.as_deref()
    }

    /// Return one named route parameter, if the route template declares it.
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }

    /// Return all path parameters for this route.
    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.path_params
    }

    /// Return a request header if its name is valid and present.
    pub fn header(&self, name: &str) -> Option<&HeaderValue> {
        let name = HeaderName::from_bytes(name.as_bytes()).ok()?;
        self.headers.get(name)
    }

    /// Return the buffered request body.
    pub fn body(&self) -> &[u8] {
        self.body.as_ref()
    }

    /// Deserialize a JSON request body. Parser details are not exposed to clients.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, HttpError> {
        serde_json::from_slice(&self.body)
            .map_err(|_| HttpError::bad_request("Invalid JSON request body"))
    }

    /// Deserialize URL-encoded query parameters into a caller-defined type.
    pub fn query<T: DeserializeOwned>(&self) -> Result<T, HttpError> {
        serde_urlencoded::from_str(self.query_string.as_deref().unwrap_or(""))
            .map_err(|_| HttpError::bad_request("Invalid query parameters"))
    }
}

/// An HTTP response independent of Axum's response/body type.
#[derive(Debug, Clone)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Response {
    /// Create an empty response with the supplied status.
    pub fn status(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    /// Create a response with an arbitrary body and status.
    pub fn new(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        }
    }

    /// Create a UTF-8 plain-text response.
    pub fn text(body: impl Into<String>) -> Self {
        Self::text_with_status(StatusCode::OK, body)
    }

    /// Create a UTF-8 HTML response. The supplied string is emitted unchanged;
    /// prefer html_view for structural views that require safe serialization.
    pub fn html(body: impl Into<String>) -> Self {
        let mut response = Self::new(StatusCode::OK, body.into());
        response.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        response
    }

    /// Serialize a value as JSON and set the JSON content type.
    pub fn json<T: Serialize>(value: &T) -> Result<Self, HttpError> {
        let body = serde_json::to_vec(value).map_err(|_| HttpError::internal_server_error())?;
        let mut response = Self::new(StatusCode::OK, Bytes::from(body));
        response.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        Ok(response)
    }

    /// Redirect using the 303 See Other status.
    pub fn redirect(location: &str) -> Result<Self, HttpError> {
        Self::redirect_with_status(StatusCode::SEE_OTHER, location)
    }

    /// Redirect using an explicit redirect status and a validated location.
    ///
    /// Only conventional redirect statuses and relative/http/https locations
    /// are accepted; javascript: and other schemes are rejected.
    pub fn redirect_with_status(status: StatusCode, location: &str) -> Result<Self, HttpError> {
        if !matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ) || !is_safe_redirect_location(location)
        {
            return Err(HttpError::bad_request("Invalid redirect target or status"));
        }
        let value = HeaderValue::from_str(location)
            .map_err(|_| HttpError::bad_request("Invalid redirect target"))?;
        let mut response = Self::status(status);
        response.headers.insert(http::header::LOCATION, value);
        Ok(response)
    }

    /// Create a download response without reading from the filesystem.
    ///
    /// The filename is reduced to ASCII letters, digits, dots, underscores and
    /// hyphens, so it cannot inject a header or disclose a local path.
    pub fn file_download(
        filename: &str,
        content_type: HeaderValue,
        contents: impl Into<Bytes>,
    ) -> Self {
        let safe_name = filename
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            })
            .collect::<String>();
        let safe_name = if safe_name.is_empty() {
            "download"
        } else {
            &safe_name
        };
        let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{safe_name}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"download\""));
        let mut response = Self::new(StatusCode::OK, contents);
        response
            .headers
            .insert(http::header::CONTENT_TYPE, content_type);
        response
            .headers
            .insert(http::header::CONTENT_DISPOSITION, disposition);
        response
    }

    /// Add or replace a response header.
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Return the status code.
    pub fn status_code(&self) -> StatusCode {
        self.status
    }

    /// Return response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Return the response body bytes.
    pub fn body(&self) -> &[u8] {
        self.body.as_ref()
    }

    fn text_with_status(status: StatusCode, body: impl Into<String>) -> Self {
        let mut response = Self::new(status, body.into());
        response.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        response
    }

    fn into_axum_response(self) -> AxumResponse {
        let mut response = AxumResponse::new(Body::from(self.body));
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        response
    }
}

/// An application-facing handler error mapped to a safe HTTP response.
///
/// The message is public output; do not put SQL, secrets, filesystem paths or
/// stack traces in it. Use internal_server_error for unexpected failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    status: StatusCode,
    public_message: String,
}

impl HttpError {
    /// Create an error with an explicit client-facing status and message.
    pub fn new(status: StatusCode, public_message: impl Into<String>) -> Self {
        Self {
            status,
            public_message: public_message.into(),
        }
    }

    /// A 400 Bad Request error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// A 404 Not Found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// A generic 500 response that does not leak internal error details.
    pub fn internal_server_error() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }

    fn payload_too_large() -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "Request body is too large")
    }

    fn bad_body() -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Could not read request body")
    }

    fn gateway_timeout() -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, "Request timed out")
    }

    fn into_response(self) -> Response {
        Response::text_with_status(self.status, self.public_message)
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.public_message)
    }
}
impl Error for HttpError {}

/// Render a structural view as a safe HTML response using rustflow-ssr.
pub fn html_view(view: &View) -> Result<Response, HttpError> {
    let html =
        rustflow_ssr::render_to_string(view).map_err(|_| HttpError::internal_server_error())?;
    Ok(Response::html(html))
}

/// Render a structural view and resolve component descriptors through the registry.
pub fn html_view_with_components(
    view: &View,
    registry: &ComponentRegistry,
) -> Result<Response, HttpError> {
    let html = rustflow_ssr::render_with_components(view, registry)
        .map_err(|_| HttpError::internal_server_error())?;
    Ok(Response::html(html))
}

/// Configuration for request buffering, handler timeouts, and concurrency.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    max_body_bytes: usize,
    request_timeout: Duration,
    max_concurrent_requests: usize,
}

impl ServerConfig {
    /// Create config with safe initial defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum buffered request body size; zero is clamped to one byte.
    pub fn with_max_body_bytes(mut self, limit: usize) -> Self {
        self.max_body_bytes = limit.max(1);
        self
    }

    /// Set the maximum time spent reading a body and executing a route.
    /// Zero is clamped to one millisecond.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout.max(Duration::from_millis(1));
        self
    }

    /// Set the maximum number of concurrently processed requests; zero is clamped.
    pub fn with_max_concurrent_requests(mut self, limit: usize) -> Self {
        self.max_concurrent_requests = limit.max(1);
        self
    }

    /// Return the configured body limit.
    pub fn max_body_bytes(&self) -> usize {
        self.max_body_bytes
    }

    /// Return the configured request timeout.
    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Return the configured concurrency cap.
    pub fn max_concurrent_requests(&self) -> usize {
        self.max_concurrent_requests
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_body_bytes: 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            max_concurrent_requests: 256,
        }
    }
}

/// Failures while assembling or starting a RustFlow server.
#[derive(Debug)]
pub enum ServerError {
    InvalidRoute(String),
    InvalidRouteName(String),
    DuplicateRoute { method: HttpMethod, path: String },
    DuplicateRouteName(String),
    ConflictingRoutePattern { existing: String, requested: String },
    MiddlewareRegistryPoisoned,
    Io(std::io::Error),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoute(path) => write!(f, "invalid route path: {path}"),
            Self::InvalidRouteName(name) => write!(f, "invalid route name: {name}"),
            Self::DuplicateRoute { method, path } => {
                write!(f, "route already registered for {method:?} {path}")
            }
            Self::DuplicateRouteName(name) => write!(f, "route name is already registered: {name}"),
            Self::ConflictingRoutePattern {
                existing,
                requested,
            } => write!(
                f,
                "route pattern {requested} conflicts with existing pattern {existing}"
            ),
            Self::MiddlewareRegistryPoisoned => write!(f, "middleware registry is unavailable"),
            Self::Io(error) => write!(f, "server I/O failed: {error}"),
        }
    }
}
impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// HTTP application with explicit route registration and bounded request handling.
///
/// Handlers are plain async Rust closures and receive RustFlow's Request type.
/// Captured state is injected by closure capture; there is no hidden service locator.
pub struct Application {
    routes: HashMap<String, MethodRouter<()>>,
    registered: HashSet<(String, HttpMethod)>,
    route_shapes: HashMap<String, String>,
    route_names: HashMap<(String, HttpMethod), String>,
    registered_names: HashSet<String>,
    middleware: Arc<RwLock<Vec<Middleware>>>,
    config: ServerConfig,
}

impl Application {
    /// Create an application with default body, timeout, and concurrency limits.
    pub fn new() -> Self {
        Self::with_config(ServerConfig::default())
    }

    /// Create an application with explicit server limits.
    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            routes: HashMap::new(),
            registered: HashSet::new(),
            route_shapes: HashMap::new(),
            route_names: HashMap::new(),
            registered_names: HashSet::new(),
            middleware: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// Return the server configuration.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Add a middleware to the global pipeline. Middleware may be registered before
    /// or after routes; the shared pipeline snapshot is taken for each request.
    pub fn middleware(&mut self, middleware: Middleware) -> Result<&mut Self, ServerError> {
        self.middleware
            .write()
            .map_err(|_| ServerError::MiddlewareRegistryPoisoned)?
            .push(middleware);
        Ok(self)
    }

    /// Return route metadata in deterministic method/path order.
    pub fn routes(&self) -> Vec<RouteInfo> {
        let mut routes = self
            .registered
            .iter()
            .map(|(path, method)| RouteInfo {
                method: *method,
                path: path.clone(),
                name: self.route_names.get(&(path.clone(), *method)).cloned(),
            })
            .collect::<Vec<_>>();
        routes.sort();
        routes
    }

    /// Register a GET handler.
    pub fn get<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Get, path, handler)
    }

    /// Register a POST handler.
    pub fn post<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Post, path, handler)
    }

    /// Register a PUT handler.
    pub fn put<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Put, path, handler)
    }

    /// Register a PATCH handler.
    pub fn patch<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Patch, path, handler)
    }

    /// Register a DELETE handler.
    pub fn delete<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Delete, path, handler)
    }

    /// Register a HEAD handler.
    pub fn head<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Head, path, handler)
    }

    /// Register an OPTIONS handler.
    pub fn options<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Options, path, handler)
    }

    /// Register a route for a supported HTTP method.
    pub fn route<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.register_route(method, path.into(), None, handler)
    }

    /// Register a uniquely named route for reverse lookup and diagnostics.
    pub fn route_named<F, Fut>(
        &mut self,
        name: impl Into<String>,
        method: HttpMethod,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        let name = name.into();
        validate_route_name(&name)?;
        self.register_route(method, path.into(), Some(name), handler)
    }

    fn register_route<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: String,
        name: Option<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        validate_route_path(&path)?;
        let shape = route_shape(&path);
        if let Some(existing) = self.route_shapes.get(&shape) {
            if existing != &path {
                return Err(ServerError::ConflictingRoutePattern {
                    existing: existing.clone(),
                    requested: path.clone(),
                });
            }
        }
        if self.registered.contains(&(path.clone(), method)) {
            return Err(ServerError::DuplicateRoute {
                method,
                path: path.clone(),
            });
        }
        if let Some(name) = name.as_ref() {
            if self.registered_names.contains(name) {
                return Err(ServerError::DuplicateRouteName(name.clone()));
            }
        }

        let handler: SharedHandler = Arc::new(move |request| Box::pin(handler(request)));
        let config = self.config.clone();
        let middleware_stack = self.middleware.clone();
        let adapter = move |Path(path_params): Path<HashMap<String, String>>,
                            request: AxumRequest| {
            let handler = handler.clone();
            let config = config.clone();
            let middleware_stack = middleware_stack.clone();
            async move {
                match tokio::time::timeout(
                    config.request_timeout,
                    dispatch_request(
                        handler,
                        path_params,
                        request,
                        config.clone(),
                        middleware_stack,
                    ),
                )
                .await
                {
                    Ok(response) => response,
                    Err(_) => HttpError::gateway_timeout()
                        .into_response()
                        .into_axum_response(),
                }
            }
        };

        let methods = self
            .routes
            .remove(&path)
            .unwrap_or_default()
            .on(method.filter(), adapter);
        self.routes.insert(path.clone(), methods);
        self.registered.insert((path.clone(), method));
        self.route_shapes.entry(shape).or_insert(path.clone());
        if let Some(name) = name {
            self.registered_names.insert(name.clone());
            self.route_names.insert((path, method), name);
        }
        Ok(self)
    }

    /// Build a bound TCP listener and run this application until it stops.
    pub async fn serve(self, address: SocketAddr) -> Result<(), ServerError> {
        let listener = TcpListener::bind(address).await.map_err(ServerError::Io)?;
        self.serve_listener(listener).await
    }

    /// Serve the application on a caller-created listener (useful for deployment adapters).
    pub async fn serve_listener(self, listener: TcpListener) -> Result<(), ServerError> {
        let router = self.into_router();
        axum::serve(listener, router).await.map_err(ServerError::Io)
    }

    /// Bind an address and serve until the caller-provided shutdown future resolves.
    pub async fn serve_with_shutdown<F>(
        self,
        address: SocketAddr,
        shutdown: F,
    ) -> Result<(), ServerError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind(address).await.map_err(ServerError::Io)?;
        self.serve_listener_with_shutdown(listener, shutdown).await
    }

    /// Serve on an existing listener and drain active connections on shutdown.
    pub async fn serve_listener_with_shutdown<F>(
        self,
        listener: TcpListener,
        shutdown: F,
    ) -> Result<(), ServerError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let router = self.into_router();
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(ServerError::Io)
    }

    fn into_router(self) -> Router {
        let Self { routes, config, .. } = self;
        let mut router = Router::new();
        for (path, methods) in routes {
            router = router.route(&path, methods);
        }
        router.layer(GlobalConcurrencyLimitLayer::new(
            config.max_concurrent_requests,
        ))
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

async fn dispatch_request(
    handler: SharedHandler,
    path_params: HashMap<String, String>,
    request: AxumRequest,
    config: ServerConfig,
    middleware_registry: Arc<RwLock<Vec<Middleware>>>,
) -> AxumResponse {
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, config.max_body_bytes).await {
        Ok(body) => body,
        Err(error) => {
            let too_large = error_is_length_limit(&error);
            let response = if too_large {
                HttpError::payload_too_large()
            } else {
                HttpError::bad_body()
            };
            return response.into_response().into_axum_response();
        }
    };
    let request = Request::new(parts.method, parts.uri, path_params, parts.headers, body);
    let middleware = match middleware_registry.read() {
        Ok(middleware) => Arc::new(middleware.clone()),
        Err(_) => return HttpError::internal_server_error().into_response().into_axum_response(),
    };
    let next = Next {
        index: 0,
        middleware,
        terminal: handler,
    };
    match next.run(request).await {
        Ok(response) => response.into_axum_response(),
        Err(error) => error.into_response().into_axum_response(),
    }
}

fn error_is_length_limit(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error.is::<LengthLimitError>() {
            return true;
        }
        current = error.source();
    }
    false
}

fn validate_route_path(path: &str) -> Result<(), ServerError> {
    if !path.starts_with('/')
        || path.contains(['?', '#'])
        || path.contains("//")
        || path.contains(' ')
        || path.contains('\\')
    {
        return Err(ServerError::InvalidRoute(path.to_owned()));
    }
    let segments = path.split('/').skip(1).collect::<Vec<_>>();
    for (index, segment) in segments.iter().enumerate() {
        if segment.contains(['{', '}']) {
            if !(segment.starts_with('{') && segment.ends_with('}')) || segment.len() < 3 {
                return Err(ServerError::InvalidRoute(path.to_owned()));
            }
            let name = &segment[1..segment.len() - 1];
            let (name, wildcard) = name
                .strip_prefix('*')
                .map_or((name, false), |name| (name, true));
            if name.is_empty()
                || !name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                || (wildcard && index + 1 != segments.len())
            {
                return Err(ServerError::InvalidRoute(path.to_owned()));
            }
        }
    }
    Ok(())
}

fn validate_route_name(name: &str) -> Result<(), ServerError> {
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        return Err(ServerError::InvalidRouteName(name.to_owned()));
    }
    Ok(())
}

fn route_shape(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.starts_with("{*") && segment.ends_with('}') {
                "{*}"
            } else if segment.starts_with('{') && segment.ends_with('}') {
                "{}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_safe_redirect_location(location: &str) -> bool {
    if location.is_empty() || location.chars().any(char::is_control) || location.starts_with("//") {
        return false;
    }
    let normalized = location.trim().to_ascii_lowercase();
    if let Some(colon) = normalized.find(':') {
        let delimiter = normalized.find(['/', '?', '#']);
        if delimiter.is_some_and(|delimiter| delimiter < colon) {
            return false;
        }
        let scheme = &normalized[..colon];
        return matches!(scheme, "http" | "https") && location[colon + 1..].starts_with("//");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        Application, HttpMethod, Middleware, Response, RouteInfo, ServerConfig, ServerError, View,
        html_view,
    };
    use axum::{
        body::{Body, to_bytes},
        http::{Request as AxumHttpRequest, StatusCode},
    };
    use serde::Deserialize;
    use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Duration};
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::oneshot};
    use tower::ServiceExt;

    async fn send(app: Application, request: AxumHttpRequest<Body>) -> http::Response<Body> {
        app.into_router()
            .oneshot(request)
            .await
            .expect("router responses do not fail at the Tower service boundary")
    }

    async fn body_text(response: http::Response<Body>) -> String {
        String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body is readable")
                .to_vec(),
        )
        .expect("test response is UTF-8")
    }

    #[derive(Deserialize)]
    struct Search {
        term: String,
    }

    #[test]
    fn route_registration_rejects_duplicate_methods_and_ambiguous_shapes() {
        let mut app = Application::new();
        app.get("/users/{id}", |_request| async { Ok(Response::text("ok")) })
            .expect("valid route");
        assert!(matches!(
            app.get("/users/{id}", |_request| async {
                Ok(Response::text("again"))
            }),
            Err(ServerError::DuplicateRoute { .. })
        ));
        assert!(matches!(
            app.get("/users/{name}", |_request| async {
                Ok(Response::text("conflict"))
            }),
            Err(ServerError::ConflictingRoutePattern { .. })
        ));
        assert!(matches!(
            app.get("/bad?query=1", |_request| async {
                Ok(Response::text("bad"))
            }),
            Err(ServerError::InvalidRoute(_))
        ));
    }

    #[tokio::test]
    async fn path_and_query_parameters_are_available_to_handlers() {
        let mut app = Application::new();
        app.get("/users/{id}", |request| async move {
            let Search { term } = request.query::<Search>()?;
            Ok(Response::text(format!(
                "user={} term={term}",
                request.path_param("id").unwrap_or("missing")
            )))
        })
        .expect("route registration");
        let request = AxumHttpRequest::builder()
            .uri("/users/42?term=rust")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, "user=42 term=rust");
    }

    #[tokio::test]
    async fn post_json_is_decoded_and_json_responses_are_typed() {
        let mut app = Application::new();
        app.post("/echo", |request| async move {
            #[derive(Deserialize, serde::Serialize)]
            struct Message {
                message: String,
            }
            let message: Message = request.json()?;
            Response::json(&message)
        })
        .expect("route registration");
        let request = AxumHttpRequest::builder()
            .method("POST")
            .uri("/echo")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"message":"hello"}"#))
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "application/json; charset=utf-8"
        );
        assert_eq!(body_text(response).await, r#"{"message":"hello"}"#);
    }

    #[tokio::test]
    async fn body_limit_returns_413_and_unknown_routes_return_404() {
        let mut app = Application::with_config(ServerConfig::new().with_max_body_bytes(4));
        app.post("/echo", |request| async move {
            Ok(Response::new(StatusCode::OK, request.body().to_vec()))
        })
        .expect("route registration");
        let request = AxumHttpRequest::builder()
            .method("POST")
            .uri("/echo")
            .body(Body::from("12345"))
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let app = Application::new();
        let request = AxumHttpRequest::builder()
            .uri("/not-found")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn request_timeout_returns_504() {
        let config = ServerConfig::new().with_request_timeout(Duration::from_millis(10));
        let mut app = Application::with_config(config);
        app.get("/slow", |_request| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(Response::text("too late"))
        })
        .expect("route registration");
        let request = AxumHttpRequest::builder()
            .uri("/slow")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn middleware_wraps_handlers_and_can_be_registered_after_routes() {
        let mut app = Application::new();
        app.get("/middleware", |_request| async { Ok(Response::text("wrapped")) })
            .expect("route registration");

        let visits = Arc::new(AtomicUsize::new(0));
        let middleware_visits = visits.clone();
        app.middleware(Middleware::new(move |request, next| {
            let visits = middleware_visits.clone();
            async move {
                visits.fetch_add(1, Ordering::SeqCst);
                let response = next.run(request).await?;
                Ok(response.with_header(
                    http::HeaderName::from_static("x-rustflow-middleware"),
                    http::HeaderValue::from_static("applied"),
                ))
            }
        }))
        .expect("middleware registration");

        let request = AxumHttpRequest::builder()
            .uri("/middleware")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-rustflow-middleware"], "applied");
        assert_eq!(body_text(response).await, "wrapped");
        assert_eq!(visits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn named_routes_are_introspectable_and_route_names_are_unique() {
        let mut app = Application::new();
        app.route_named("users.show", HttpMethod::Get, "/users/{id}", |_request| async {
            Ok(Response::text("user"))
        })
        .expect("named route");
        app.get("/health", |_request| async { Ok(Response::text("ok")) })
            .expect("unnamed route");

        let routes = app.routes();
        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&RouteInfo {
            method: HttpMethod::Get,
            path: "/users/{id}".to_owned(),
            name: Some("users.show".to_owned()),
        }));
        assert!(routes.contains(&RouteInfo {
            method: HttpMethod::Get,
            path: "/health".to_owned(),
            name: None,
        }));
        assert!(matches!(
            app.route_named("users.show", HttpMethod::Get, "/other", |_request| async {
                Ok(Response::text("other"))
            }),
            Err(ServerError::DuplicateRouteName(_))
        ));
        assert!(matches!(
            app.route_named("invalid name", HttpMethod::Get, "/bad-name", |_request| async {
                Ok(Response::text("bad"))
            }),
            Err(ServerError::InvalidRouteName(_))
        ));
    }

    #[tokio::test]
    async fn listener_serves_http_and_supports_graceful_shutdown() {
        let mut app = Application::new();
        app.get("/health", |_request| async { Ok(Response::text("ok")) })
            .expect("route registration");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("listener address");
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(app.serve_listener_with_shutdown(listener, async move {
            let _ = shutdown_rx.await;
        }));

        let mut stream = TcpStream::connect(address).await.expect("TCP connect");
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("write request");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read response");
        let response = String::from_utf8(bytes).expect("HTTP response is UTF-8");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("ok"));

        shutdown_tx.send(()).expect("shutdown signal");
        server.await.expect("server task").expect("graceful server stop");
    }

    #[tokio::test]
    async fn method_not_allowed_and_safe_redirect_are_explicit() {
        let mut app = Application::new();
        app.get("/items", |_request| async { Ok(Response::text("items")) })
            .expect("route registration");
        let request = AxumHttpRequest::builder()
            .method("POST")
            .uri("/items")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        assert!(Response::redirect("https://example.com").is_ok());
        assert!(Response::redirect("/login").is_ok());
        assert!(Response::redirect("javascript:alert(1)").is_err());
        assert!(Response::redirect("//evil.example").is_err());
    }

    #[test]
    fn html_helper_uses_safe_view_serialization() {
        let view = View::element("main", [View::text("<safe> & escaped")]).expect("view");
        let response = html_view(&view).expect("safe rendering");
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.body(), b"<main>&lt;safe&gt; &amp; escaped</main>");
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        let unsafe_view =
            View::element("script", [View::text("alert(1)")]).expect("structural node");
        assert!(html_view(&unsafe_view).is_err());
    }
}
