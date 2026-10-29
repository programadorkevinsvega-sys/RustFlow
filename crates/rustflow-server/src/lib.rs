//! RustFlow-owned HTTP request, response, routing, and server composition APIs.
//!
//! Axum, Tokio, and Tower are isolated to this adapter crate. Core reactivity and
//! the structural view model do not depend on HTTP infrastructure.

mod config;
mod csrf;
mod group;
mod page;
mod proxy;
mod sessions;
mod tls;
mod validation;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::extract::ConnectInfo;
use axum::middleware::{self, Next as AxumNext};
use axum::serve::ListenerExt;
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::{Path, Request as AxumRequest, State},
    response::Response as AxumResponse,
    routing::{MethodFilter, MethodRouter},
};

use bytes::Bytes;
pub use config::{AppConfig, AppEnvironment, ConfigError};
pub use csrf::{CsrfError, CsrfProtection, CsrfToken};
pub use group::RouteGroup;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use http_body_util::LengthLimitError;
pub use page::HtmlPage;
pub use proxy::{ClientAddressPolicy, IpNetwork};
use serde::{Serialize, de::DeserializeOwned};
pub use sessions::{
    AccessPolicy, AccessPolicyError, AuthIdentity, InMemorySessionStore, PasswordHash, RolePolicy,
    SameSite, SessionContext, SessionCookieConfig, SessionError, SessionId, SessionManager,
    SessionStore, require_authentication, require_permission,
};
pub use tls::tls_server_config_from_pem;
use tokio::net::TcpListener;
use tower::limit::GlobalConcurrencyLimitLayer;
use tracing::Instrument;
pub use validation::{ValidateInput, ValidationErrors};

use rustflow::components::{ComponentRegistry, SharedComponentCatalog};
use rustflow_view::{Element, View};

/// Future returned by a RustFlow route handler.
pub type HandlerFuture =
    Pin<Box<dyn Future<Output = Result<Response, HttpError>> + Send + 'static>>;
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

type SharedHandler = Arc<dyn Fn(Request) -> HandlerFuture + Send + Sync + 'static>;

type SharedMiddleware = Arc<dyn Fn(Request, Next) -> HandlerFuture + Send + Sync + 'static>;

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
    request_id: String,
    peer_addr: Option<SocketAddr>,
    client_ip: Option<IpAddr>,
    https: bool,
    session: Option<SessionContext>,
}

struct RequestInit {
    method: Method,
    uri: Uri,
    path_params: HashMap<String, String>,
    headers: HeaderMap,
    body: Bytes,
    request_id: String,
    identity: proxy::ClientIdentity,
    peer_addr: Option<SocketAddr>,
}

impl Request {
    fn new(init: RequestInit) -> Self {
        Self {
            method: init.method,
            path: init.uri.path().to_owned(),
            query_string: init.uri.query().map(str::to_owned),
            path_params: init.path_params,
            headers: init.headers,
            body: init.body,
            request_id: init.request_id,
            peer_addr: init.peer_addr,
            client_ip: init.identity.client_ip,
            https: init.identity.https,
            session: None,
        }
    }

    /// Return the opaque request identifier assigned at the HTTP boundary.
    ///
    /// It is intended for log/trace correlation only, never for authorization.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Return the TCP peer address when the connection supplied one.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }

    /// Return the client IP after applying the configured proxy-trust policy.
    pub fn client_ip(&self) -> Option<IpAddr> {
        self.client_ip
    }

    /// Return whether the request is treated as HTTPS (TLS connection or trusted proto).
    pub fn is_https(&self) -> bool {
        self.https
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

    /// Deserialize an `application/x-www-form-urlencoded` body into a typed value.
    ///
    /// Unsupported media types return 415 and malformed/incorrectly typed values
    /// return a generic 400. The server body-size limit applies before this method
    /// can be called.
    pub fn form<T: DeserializeOwned>(&self) -> Result<T, HttpError> {
        let content_type = self
            .header("content-type")
            .and_then(|value| value.to_str().ok());
        if !content_type.is_some_and(|value| {
            value.split(';').next().is_some_and(|media_type| {
                media_type
                    .trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
            })
        }) {
            return Err(HttpError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Expected application/x-www-form-urlencoded content",
            ));
        }

        let body = std::str::from_utf8(self.body())
            .map_err(|_| HttpError::bad_request("Invalid form input"))?;
        serde_urlencoded::from_str(body).map_err(|_| HttpError::bad_request("Invalid form input"))
    }

    /// Deserialize and validate a URL-encoded HTML form body.
    pub fn validated_form<T: DeserializeOwned + ValidateInput>(&self) -> Result<T, HttpError> {
        let value = self.form::<T>()?;
        value.validate().map_err(HttpError::validation_failed)?;
        Ok(value)
    }

    /// Deserialize and validate a JSON body.
    pub fn validated_json<T: DeserializeOwned + ValidateInput>(&self) -> Result<T, HttpError> {
        let value = self.json::<T>()?;
        value.validate().map_err(HttpError::validation_failed)?;
        Ok(value)
    }

    /// Deserialize and validate typed query parameters.
    pub fn validated_query<T: DeserializeOwned + ValidateInput>(&self) -> Result<T, HttpError> {
        let value = self.query::<T>()?;
        value.validate().map_err(HttpError::validation_failed)?;
        Ok(value)
    }

    /// Redirect to a validated same-origin Referer path, or to a local fallback.
    ///
    /// Absolute Referer values are accepted only when scheme, host, and effective
    /// port match this request. Missing/invalid/cross-origin referrers use the
    /// fallback. The fallback must be an origin-relative path, never an absolute
    /// URL or a protocol-relative URL.
    pub fn redirect_back(&self, fallback: &str) -> Result<Response, HttpError> {
        let fallback = safe_local_redirect_target(fallback)
            .ok_or_else(|| HttpError::bad_request("Invalid local redirect target"))?;
        if let Some(target) = self
            .header("referer")
            .and_then(|value| value.to_str().ok())
            .and_then(|referer| self.same_origin_referer_target(referer))
        {
            return Response::redirect(&target);
        }
        Response::redirect(&fallback)
    }

    /// Require an explicit same-origin browser context for cookie-authenticated mutations.
    ///
    /// An invalid or cross-origin Origin header fails closed rather than falling back
    /// to Referer. When Origin is absent, a same-origin Referer is accepted.
    pub(crate) fn has_same_origin_context(&self) -> bool {
        if let Some(origin) = self.header("origin") {
            let Ok(origin) = origin.to_str() else {
                return false;
            };
            let Ok(uri) = origin.parse::<Uri>() else {
                return false;
            };
            return uri.path() == "/" && uri.query().is_none() && self.is_same_origin_uri(&uri);
        }

        self.header("referer")
            .and_then(|value| value.to_str().ok())
            .and_then(|referer| referer.parse::<Uri>().ok())
            .is_some_and(|uri| self.is_same_origin_uri(&uri))
    }

    fn is_same_origin_uri(&self, uri: &Uri) -> bool {
        let Some(scheme) = uri.scheme_str() else {
            return false;
        };
        let expected_scheme = if self.is_https() { "https" } else { "http" };
        if !scheme.eq_ignore_ascii_case(expected_scheme) {
            return false;
        }
        let Some(remote) = uri.authority() else {
            return false;
        };
        if remote.as_str().contains('@') {
            return false;
        }
        let Some(host_header) = self.header("host").and_then(|value| value.to_str().ok()) else {
            return false;
        };
        let Ok(local) = host_header.parse::<http::uri::Authority>() else {
            return false;
        };
        if !remote.host().eq_ignore_ascii_case(local.host()) {
            return false;
        }
        let default_port = if self.is_https() { 443 } else { 80 };
        remote.port_u16().unwrap_or(default_port) == local.port_u16().unwrap_or(default_port)
    }

    fn same_origin_referer_target(&self, referer: &str) -> Option<String> {
        let uri = referer.parse::<Uri>().ok()?;
        if !self.is_same_origin_uri(&uri) {
            return None;
        }
        let target = uri.path_and_query()?.as_str();
        safe_local_redirect_target(target)
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
    validation_errors: Option<ValidationErrors>,
}

#[derive(Serialize)]
struct ValidationErrorBody {
    error: &'static str,
    errors: ValidationErrors,
}

impl HttpError {
    /// Create an error with an explicit client-facing status and message.
    pub fn new(status: StatusCode, public_message: impl Into<String>) -> Self {
        Self {
            status,
            public_message: public_message.into(),
            validation_errors: None,
        }
    }

    /// A 403 Forbidden error.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    fn validation_failed(errors: ValidationErrors) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            public_message: "Request validation failed".to_owned(),
            validation_errors: Some(errors),
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

    fn https_required() -> Self {
        Self::new(StatusCode::BAD_REQUEST, "HTTPS is required")
    }

    fn into_response(self) -> Response {
        if let Some(errors) = self.validation_errors {
            let payload = ValidationErrorBody {
                error: "validation_failed",
                errors,
            };
            if let Ok(body) = serde_json::to_vec(&payload) {
                let mut response = Response::new(self.status, Bytes::from(body));
                response.headers.insert(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json; charset=utf-8"),
                );
                return response;
            }
            return Response::text_with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            );
        }
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

/// Render a structural view using a shareable immutable component catalog.
pub fn html_view_with_shared_components(
    view: &View,
    catalog: &SharedComponentCatalog,
) -> Result<Response, HttpError> {
    let html = rustflow_ssr::render_with_shared_components(view, catalog)
        .map_err(|_| HttpError::internal_server_error())?;
    Ok(Response::html(html))
}

/// Render a complete HTML document from a title, language, and structural body.
///
/// The document shell and title are built as structural nodes; title, language,
/// attributes, and body text are escaped/validated by the safe SSR serializer.
/// No caller-supplied string is inserted as raw markup.
pub fn html_document(title: &str, lang: &str, body: &View) -> Result<Response, HttpError> {
    render_html_document(title, lang, body, None, None)
}

/// Render a complete HTML document while resolving components through a local registry.
pub fn html_document_with_components(
    title: &str,
    lang: &str,
    body: &View,
    registry: &ComponentRegistry,
) -> Result<Response, HttpError> {
    render_html_document(title, lang, body, Some(registry), None)
}

/// Render a complete HTML document using a shareable immutable component catalog.
pub fn html_document_with_shared_components(
    title: &str,
    lang: &str,
    body: &View,
    catalog: &SharedComponentCatalog,
) -> Result<Response, HttpError> {
    render_html_document(title, lang, body, None, Some(catalog))
}

/// Render a complete HTML document with opt-in hydration boundaries and a safe JSON payload.
///
/// The body HTML is still produced by the SSR serializer. The payload is a validated
/// `HydrationData` value and is emitted as inert `application/json` script text; this function
/// does not load a client bundle or claim that the page has been hydrated.
pub fn html_document_with_hydration_data(
    title: &str,
    lang: &str,
    body: &View,
    data: &rustflow_ssr::HydrationData,
) -> Result<Response, HttpError> {
    render_hydratable_html_document(title, lang, body, None, None, data)
}

/// Hydratable HTML document with component descriptors resolved through a local registry.
pub fn html_document_with_components_and_hydration_data(
    title: &str,
    lang: &str,
    body: &View,
    registry: &ComponentRegistry,
    data: &rustflow_ssr::HydrationData,
) -> Result<Response, HttpError> {
    render_hydratable_html_document(title, lang, body, Some(registry), None, data)
}

/// Hydratable HTML document using a shareable immutable component catalog.
pub fn html_document_with_shared_components_and_hydration_data(
    title: &str,
    lang: &str,
    body: &View,
    catalog: &SharedComponentCatalog,
    data: &rustflow_ssr::HydrationData,
) -> Result<Response, HttpError> {
    render_hydratable_html_document(title, lang, body, None, Some(catalog), data)
}

fn render_hydratable_html_document(
    title: &str,
    lang: &str,
    body: &View,
    registry: Option<&ComponentRegistry>,
    shared_catalog: Option<&SharedComponentCatalog>,
    data: &rustflow_ssr::HydrationData,
) -> Result<Response, HttpError> {
    let rendered_body = match (registry, shared_catalog) {
        (Some(registry), _) => rustflow_ssr::HtmlRenderer::with_components(registry)
            .with_hydration_boundaries()
            .render(body),
        (_, Some(catalog)) => rustflow_ssr::HtmlRenderer::with_shared_components(catalog)
            .with_hydration_boundaries()
            .render(body),
        (None, None) => rustflow_ssr::HtmlRenderer::new()
            .with_hydration_boundaries()
            .render(body),
    }
    .map_err(|_| HttpError::internal_server_error())?;

    // Build the document shell through the normal safe serializer. Only the serializer-produced
    // body fragment and the script-context-safe HydrationData string are inserted below.
    let shell = html_document(title, lang, &View::fragment([]))?;
    let shell_html =
        std::str::from_utf8(shell.body()).map_err(|_| HttpError::internal_server_error())?;
    let closing_body = shell_html
        .rfind("</body>")
        .ok_or_else(HttpError::internal_server_error)?;
    let mut document = shell_html.to_owned();
    document.insert_str(
        closing_body,
        &format!(
            "{rendered_body}<script type=\"application/json\" id=\"rustflow-hydration-data\">{}</script>",
            data.as_str()
        ),
    );
    Ok(Response::html(document))
}

fn render_html_document(
    title: &str,
    lang: &str,
    body: &View,
    registry: Option<&ComponentRegistry>,
    shared_catalog: Option<&SharedComponentCatalog>,
) -> Result<Response, HttpError> {
    let title = View::element("title", [View::text(title)])
        .map_err(|_| HttpError::internal_server_error())?;
    let charset = Element::new("meta")
        .and_then(|element| element.with_attribute("charset", "utf-8"))
        .map_err(|_| HttpError::internal_server_error())?
        .build();
    let head =
        View::element("head", [charset, title]).map_err(|_| HttpError::internal_server_error())?;
    let body =
        View::element("body", [body.clone()]).map_err(|_| HttpError::internal_server_error())?;
    let root = Element::new("html")
        .and_then(|element| element.with_attribute("lang", lang))
        .map_err(|_| HttpError::internal_server_error())?
        .with_children([head, body])
        .build();
    let tree = root;
    let markup = match (registry, shared_catalog) {
        (Some(registry), _) => rustflow_ssr::render_with_components(&tree, registry),
        (_, Some(catalog)) => rustflow_ssr::render_with_shared_components(&tree, catalog),
        (None, None) => rustflow_ssr::render_to_string(&tree),
    }
    .map_err(|_| HttpError::internal_server_error())?;
    Ok(Response::html(format!("<!doctype html>{markup}")))
}

/// A custom safe HTML error page's title and visible message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPage {
    title: String,
    message: String,
}

/// Configurable HTML bodies for selected 4xx/5xx status codes.
///
/// Only error statuses are accepted. A configured page replaces the response body
/// for that status after routing/handler processing. Title and message are rendered
/// as text nodes and are never interpreted as HTML.
#[derive(Debug, Clone, Default)]
pub struct ErrorPages {
    pages: HashMap<StatusCode, ErrorPage>,
}

impl ErrorPages {
    /// Create an empty error-page map; unconfigured statuses keep their normal response.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure a page for a client or server error status.
    ///
    /// Configuring the same status more than once replaces its previous page.
    pub fn with_page(
        mut self,
        status: StatusCode,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Self, ErrorPageConfigError> {
        if !status.is_client_error() && !status.is_server_error() {
            return Err(ErrorPageConfigError::NotErrorStatus(status));
        }
        self.pages.insert(
            status,
            ErrorPage {
                title: title.into(),
                message: message.into(),
            },
        );
        Ok(self)
    }
}

/// Invalid configuration for a custom error page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorPageConfigError {
    NotErrorStatus(StatusCode),
}

impl fmt::Display for ErrorPageConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotErrorStatus(status) => {
                write!(
                    f,
                    "custom error pages require a 4xx or 5xx status, got {status}"
                )
            }
        }
    }
}

impl Error for ErrorPageConfigError {}

/// Deployment profile used to validate composition-root wiring before bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentProfile {
    /// Local and test defaults. HTTPS and proxy policy are optional.
    Development,
    /// Requires an explicit client-address policy and HTTPS wiring before serve.
    Production,
}

/// Configuration for request buffering, handler timeouts, and concurrency.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    max_body_bytes: usize,
    request_timeout: Duration,
    max_concurrent_requests: usize,
    error_pages: ErrorPages,
    deployment: DeploymentProfile,
    client_address: ClientAddressPolicy,
    require_https: bool,
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

    /// Configure safe HTML pages for selected error statuses.
    pub fn with_error_pages(mut self, error_pages: ErrorPages) -> Self {
        self.error_pages = error_pages;
        self
    }

    /// Production defaults: HTTPS required and an explicit client-address policy
    /// must be wired before `serve` or `serve_tls`.
    pub fn production() -> Self {
        Self {
            deployment: DeploymentProfile::Production,
            require_https: true,
            ..Self::default()
        }
    }

    /// Select development or production composition-root checks.
    pub fn with_deployment_profile(mut self, profile: DeploymentProfile) -> Self {
        self.deployment = profile;
        self
    }

    /// Require a TLS connection or a trusted `https` forwarded proto.
    pub fn with_require_https(mut self, require_https: bool) -> Self {
        self.require_https = require_https;
        self
    }

    /// Ignore forwarded headers and use the TCP peer as the client address.
    pub fn with_direct_client_addresses(mut self) -> Self {
        self.client_address = ClientAddressPolicy::Direct;
        self
    }

    /// Honor forwarded headers only from these reverse-proxy networks.
    pub fn with_trusted_proxies(
        mut self,
        proxies: impl IntoIterator<Item = IpNetwork>,
    ) -> Result<Self, ServerError> {
        let proxies = proxies.into_iter().collect::<Vec<_>>();
        if proxies.is_empty() {
            return Err(ServerError::EmptyTrustedProxyList);
        }
        self.client_address = ClientAddressPolicy::TrustedProxies(proxies);
        Ok(self)
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

    /// Return the selected deployment profile.
    pub fn deployment(&self) -> DeploymentProfile {
        self.deployment
    }

    /// Return whether HTTPS is required after proxy/TLS resolution.
    pub fn require_https(&self) -> bool {
        self.require_https
    }

    /// Return the client-address reconstruction policy.
    pub fn client_address_policy(&self) -> &ClientAddressPolicy {
        &self.client_address
    }

    /// Validate production wiring for plaintext `serve` / `serve_listener`.
    pub fn validate(&self) -> Result<(), ServerError> {
        self.validate_for_bind(BindKind::Plain)
    }

    /// Validate production wiring for `serve_tls` / `serve_tls_with_shutdown`.
    pub fn validate_tls(&self) -> Result<(), ServerError> {
        self.validate_for_bind(BindKind::Tls)
    }

    fn validate_for_bind(&self, bind: BindKind) -> Result<(), ServerError> {
        if let ClientAddressPolicy::TrustedProxies(proxies) = &self.client_address {
            if proxies.is_empty() {
                return Err(ServerError::EmptyTrustedProxyList);
            }
        }
        if self.deployment == DeploymentProfile::Production && self.require_https {
            let has_proxy = matches!(self.client_address, ClientAddressPolicy::TrustedProxies(_));
            if bind == BindKind::Plain && !has_proxy {
                return Err(ServerError::ProductionHttpsRequiresTlsOrTrustedProxy);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindKind {
    Plain,
    Tls,
}

#[derive(Clone, Copy)]
struct ConnectionSecure(bool);

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_body_bytes: 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            max_concurrent_requests: 256,
            error_pages: ErrorPages::default(),
            deployment: DeploymentProfile::Development,
            client_address: ClientAddressPolicy::Direct,
            require_https: false,
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
    UnknownRoute { method: HttpMethod, path: String },
    EmptyTrustedProxyList,
    InvalidIpNetwork(String),
    ProductionHttpsRequiresTlsOrTrustedProxy,
    InvalidTlsMaterial(String),
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
            Self::UnknownRoute { method, path } => {
                write!(f, "no route registered for {method:?} {path}")
            }
            Self::EmptyTrustedProxyList => {
                write!(f, "trusted proxy policy requires at least one network")
            }
            Self::InvalidIpNetwork(value) => write!(f, "invalid IP network: {value}"),
            Self::ProductionHttpsRequiresTlsOrTrustedProxy => write!(
                f,
                "production HTTPS requires serve_tls or a trusted reverse-proxy policy"
            ),
            Self::InvalidTlsMaterial(message) => write!(f, "invalid TLS material: {message}"),
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
    route_middleware: HashMap<(String, HttpMethod), Arc<RwLock<Vec<Middleware>>>>,
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
            route_middleware: HashMap::new(),
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

    /// Append middleware to one already-registered method/path.
    ///
    /// Route middleware runs after global middleware and after any group
    /// middleware captured when the route was registered.
    pub fn route_middleware(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        middleware: Middleware,
    ) -> Result<&mut Self, ServerError> {
        let path = path.into();
        let stack = self
            .route_middleware
            .get(&(path.clone(), method))
            .ok_or_else(|| ServerError::UnknownRoute {
                method,
                path: path.clone(),
            })?;
        stack
            .write()
            .map_err(|_| ServerError::MiddlewareRegistryPoisoned)?
            .push(middleware);
        Ok(self)
    }

    /// Register a nested collection of routes that share a prefix and middleware.
    pub fn group<F>(
        &mut self,
        prefix: impl Into<String>,
        configure: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: FnOnce(&mut RouteGroup<'_>) -> Result<(), ServerError>,
    {
        let prefix = group::normalize_group_prefix(&prefix.into())?;
        {
            let mut grouped = RouteGroup::new(self, prefix, String::new(), Vec::new());
            configure(&mut grouped)?;
        }
        Ok(self)
    }

    /// Validate production/proxy/TLS wiring without opening a listener.
    pub fn validate(&self) -> Result<(), ServerError> {
        self.config.validate()
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
        self.register_route(method, path.into(), None, Vec::new(), handler)
    }

    /// Register a route whose authentication guard runs before its handler.
    /// SessionManager middleware must also be installed; without it, the guard fails closed.
    pub fn route_protected<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        let path = path.into();
        self.route(method, path.clone(), handler)?;
        self.route_middleware(method, path, sessions::require_authentication())?;
        Ok(self)
    }

    /// Register a route with authorization checked before its handler.
    /// A missing policy/rule or an evaluation error never grants access.
    pub fn route_authorized<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        permission: impl Into<String>,
        policy: Option<Arc<dyn AccessPolicy>>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        let path = path.into();
        self.route(method, path.clone(), handler)?;
        self.route_middleware(
            method,
            path,
            sessions::require_permission(permission, policy),
        )?;
        Ok(self)
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
        self.register_route(method, path.into(), Some(name), Vec::new(), handler)
    }

    pub(crate) fn register_route<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: String,
        name: Option<String>,
        extra_middleware: Vec<Middleware>,
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
        let route_middleware = Arc::new(RwLock::new(extra_middleware));
        self.route_middleware
            .insert((path.clone(), method), route_middleware.clone());
        let route_template = path.clone();
        let adapter = move |Path(path_params): Path<HashMap<String, String>>,
                            mut request: AxumRequest| {
            let handler = handler.clone();
            let config = config.clone();
            let middleware_stack = middleware_stack.clone();
            let route_middleware = route_middleware.clone();
            let route_template = route_template.clone();
            let request_id = request_id_from_headers(request.headers());
            if let Ok(value) = HeaderValue::from_str(&request_id) {
                request
                    .headers_mut()
                    .insert(HeaderName::from_static("x-request-id"), value);
            }
            async move {
                match tokio::time::timeout(
                    config.request_timeout,
                    dispatch_request(
                        handler,
                        path_params,
                        request,
                        config.clone(),
                        middleware_stack,
                        route_middleware,
                        route_template,
                    ),
                )
                .await
                {
                    Ok(response) => response,
                    Err(_) => {
                        attach_request_id(HttpError::gateway_timeout().into_response(), &request_id)
                            .into_axum_response()
                    }
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
        let router = self.into_router(BindKind::Plain)?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .map_err(ServerError::Io)
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
        let router = self.into_router(BindKind::Plain)?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServerError::Io)
    }

    /// Serve HTTPS on a caller-created listener using rustls.
    pub async fn serve_tls(
        self,
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
    ) -> Result<(), ServerError> {
        self.serve_tls_with_shutdown(listener, tls, std::future::pending())
            .await
    }

    /// Serve HTTPS until the caller-provided shutdown future resolves.
    pub async fn serve_tls_with_shutdown<F>(
        self,
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
        shutdown: F,
    ) -> Result<(), ServerError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let router = self.into_router(BindKind::Tls)?;
        // `tap_io` is required so ConnectInfo<SocketAddr> is implemented for custom listeners.
        axum::serve(
            tls::TlsListener::new(listener, tls)
                .map_err(ServerError::Io)?
                .tap_io(|_| {}),
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServerError::Io)
    }

    fn into_router(self, bind: BindKind) -> Result<Router, ServerError> {
        self.config.validate_for_bind(bind)?;
        let Self { routes, config, .. } = self;
        let mut router = Router::new();
        for (path, methods) in routes {
            router = router.route(&path, methods);
        }
        Ok(router
            .layer(Extension(ConnectionSecure(matches!(bind, BindKind::Tls))))
            .layer(GlobalConcurrencyLimitLayer::new(
                config.max_concurrent_requests,
            ))
            .layer(middleware::from_fn_with_state(
                config.error_pages,
                request_metadata_and_error_pages,
            )))
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
    route_middleware: Arc<RwLock<Vec<Middleware>>>,
    route_template: String,
) -> AxumResponse {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let connection_secure = request
        .extensions()
        .get::<ConnectionSecure>()
        .map(|flag| flag.0)
        .unwrap_or(false);
    let (parts, body) = request.into_parts();
    let request_id = request_id_from_headers(&parts.headers);
    let request_id_for_response = request_id.clone();
    let method = parts.method.clone();
    let identity = proxy::resolve_client_identity(
        peer,
        &parts.headers,
        &config.client_address,
        connection_secure,
    );
    let span = tracing::info_span!(
        "http.request",
        request_id = %request_id,
        method = %method,
        route = %route_template,
    );

    async move {
        if config.require_https && !identity.https {
            tracing::warn!(status = %StatusCode::BAD_REQUEST, "HTTPS is required");
            let response = HttpError::https_required().into_response();
            return attach_request_id(response, &request_id_for_response).into_axum_response();
        }

        let body = match to_bytes(body, config.max_body_bytes).await {
            Ok(body) => body,
            Err(error) => {
                let too_large = error_is_length_limit(&error);
                let error = if too_large {
                    HttpError::payload_too_large()
                } else {
                    HttpError::bad_body()
                };
                tracing::warn!(status = %error.status, "request rejected while reading body");
                let response = error.into_response();
                return attach_request_id(response, &request_id_for_response).into_axum_response();
            }
        };

        let request = Request::new(RequestInit {
            method: parts.method,
            uri: parts.uri,
            path_params,
            headers: parts.headers,
            body,
            request_id: request_id_for_response.clone(),
            identity,
            peer_addr: peer,
        });
        let middleware = match (middleware_registry.read(), route_middleware.read()) {
            (Ok(global), Ok(scoped)) => {
                let mut stack = global.clone();
                stack.extend(scoped.iter().cloned());
                Arc::new(stack)
            }
            _ => {
                tracing::error!("middleware registry is unavailable");
                let response = HttpError::internal_server_error().into_response();
                return attach_request_id(response, &request_id_for_response).into_axum_response();
            }
        };
        let next = Next {
            index: 0,
            middleware,
            terminal: handler,
        };
        let response = match next.run(request).await {
            Ok(response) => {
                tracing::info!(status = %response.status, "request completed");
                response
            }
            Err(error) => {
                tracing::warn!(status = %error.status, "request failed");
                error.into_response()
            }
        };
        attach_request_id(response, &request_id_for_response).into_axum_response()
    }
    .instrument(span)
    .await
}

async fn request_metadata_and_error_pages(
    State(error_pages): State<ErrorPages>,
    mut request: AxumRequest,
    next: AxumNext,
) -> AxumResponse {
    let request_id = request_id_from_headers(request.headers());
    let method = request.method().clone();
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        request
            .headers_mut()
            .insert(HeaderName::from_static("x-request-id"), value);
    }

    let mut response = next.run(request).await;
    let status = response.status();
    if let Some(page) = error_pages.pages.get(&status) {
        match render_error_page(status, page) {
            Ok(rendered) => {
                let (mut parts, _) = response.into_parts();
                parts.headers.remove(http::header::CONTENT_LENGTH);
                if let Some(content_type) = rendered.headers().get(http::header::CONTENT_TYPE) {
                    parts
                        .headers
                        .insert(http::header::CONTENT_TYPE, content_type.clone());
                }
                response = AxumResponse::from_parts(parts, Body::from(rendered.body().to_vec()));
            }
            Err(_) => tracing::error!(
                request_id = %request_id,
                status = %status,
                "configured error page could not be rendered"
            ),
        }
    }

    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("rustflow-request")),
    );
    if status.is_client_error() || status.is_server_error() {
        tracing::warn!(
            request_id = %request_id,
            method = %method,
            status = %status,
            "HTTP error response"
        );
    }
    response
}

fn render_error_page(status: StatusCode, page: &ErrorPage) -> Result<Response, HttpError> {
    let heading = View::element("h1", [View::text(page.title.clone())])
        .map_err(|_| HttpError::internal_server_error())?;
    let message = View::element("p", [View::text(page.message.clone())])
        .map_err(|_| HttpError::internal_server_error())?;
    let content = View::element("main", [heading, message])
        .map_err(|_| HttpError::internal_server_error())?;
    let mut response = html_document(&page.title, "en", &content)?;
    response.status = status;
    Ok(response)
}

fn request_id_from_headers(headers: &HeaderMap) -> String {
    if let Some(candidate) = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
    {
        if is_valid_request_id(candidate) {
            return candidate.to_owned();
        }
    }

    let sequence = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("rf-{:x}-{:x}", std::process::id(), sequence)
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn attach_request_id(mut response: Response, request_id: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response
            .headers
            .insert(HeaderName::from_static("x-request-id"), value);
    }
    response
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

pub(crate) fn validate_route_path(path: &str) -> Result<(), ServerError> {
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

pub(crate) fn validate_route_name(name: &str) -> Result<(), ServerError> {
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

fn safe_local_redirect_target(target: &str) -> Option<String> {
    if target.is_empty()
        || target.starts_with("//")
        || target.starts_with("/\\")
        || target.contains('\\')
        || target.chars().any(char::is_control)
    {
        return None;
    }
    let uri = target.parse::<Uri>().ok()?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        return None;
    }
    let path_and_query = uri.path_and_query()?.as_str();
    if !path_and_query.starts_with('/') || path_and_query.starts_with("//") {
        return None;
    }
    Some(path_and_query.to_owned())
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
        Application, BindKind, ClientAddressPolicy, DeploymentProfile, ErrorPageConfigError,
        ErrorPages, HtmlPage, HttpError, HttpMethod, IpNetwork, Middleware, Response, RouteInfo,
        ServerConfig, ServerError, View, html_document, html_view,
    };
    use axum::{
        body::{Body, to_bytes},
        http::{Request as AxumHttpRequest, StatusCode},
    };
    use rustflow::components::{
        ComponentRender, SharedComponentCatalog, SharedComponentCatalogBuilder,
        SharedComponentDefinition,
    };
    use rustflow::reactivity::signal;
    use rustflow_view::{Property, PropertyValue};
    use serde::Deserialize;
    use std::{
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::oneshot,
    };
    use tower::ServiceExt;

    async fn send(app: Application, request: AxumHttpRequest<Body>) -> http::Response<Body> {
        app.into_router(BindKind::Plain)
            .expect("development config is valid")
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
    async fn request_id_is_available_to_handlers_and_echoed_safely() {
        let mut app = Application::new();
        app.get("/request-id", |request| async move {
            Ok(Response::text(request.request_id().to_owned()))
        })
        .expect("route registration");

        let request = AxumHttpRequest::builder()
            .uri("/request-id")
            .header("x-request-id", "client_trace_42")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-request-id"], "client_trace_42");
        assert_eq!(body_text(response).await, "client_trace_42");

        let mut app = Application::new();
        app.get("/request-id", |request| async move {
            Ok(Response::text(request.request_id().to_owned()))
        })
        .expect("route registration");
        let request = AxumHttpRequest::builder()
            .uri("/request-id")
            .header("x-request-id", "invalid id with spaces")
            .body(Body::empty())
            .expect("syntactically valid HTTP field");
        let response = send(app, request).await;
        let generated = response.headers()["x-request-id"]
            .to_str()
            .expect("request id is a header value")
            .to_owned();
        assert!(generated.starts_with("rf-"));
        assert_eq!(body_text(response).await, generated);
    }

    #[test]
    fn complete_html_documents_escape_title_language_and_body() {
        let body = View::element("main", [View::text("<body> & content")]).expect("body view");
        let response =
            html_document("RustFlow <admin>", "en", &body).expect("complete HTML document");
        assert_eq!(response.status_code(), StatusCode::OK);
        let html = String::from_utf8(response.body().to_vec()).expect("HTML is UTF-8");
        assert!(html.starts_with("<!doctype html><html lang=\"en\">"));
        assert!(html.contains("<title>RustFlow &lt;admin&gt;</title>"));
        assert!(html.contains("&lt;body&gt; &amp; content"));
        assert!(!html.contains("<body> & content"));
    }

    #[test]
    fn error_page_configuration_rejects_non_error_statuses() {
        assert!(matches!(
            ErrorPages::new().with_page(StatusCode::OK, "No", "No"),
            Err(ErrorPageConfigError::NotErrorStatus(StatusCode::OK))
        ));
    }

    #[tokio::test]
    async fn configured_error_pages_render_escaped_html_and_keep_request_id() {
        let pages = ErrorPages::new()
            .with_page(
                StatusCode::NOT_FOUND,
                "Missing <page>",
                "Cannot find <script>alert(1)</script>",
            )
            .expect("404 is an error status")
            .with_page(
                StatusCode::METHOD_NOT_ALLOWED,
                "Wrong method",
                "This route is read-only.",
            )
            .expect("405 is an error status");
        let config = ServerConfig::new().with_error_pages(pages);
        let mut app = Application::with_config(config.clone());
        app.get("/only-get", |_request| async { Ok(Response::text("ok")) })
            .expect("route registration");

        let request = AxumHttpRequest::builder()
            .uri("/missing")
            .header("x-request-id", "missing_case_4")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert_eq!(response.headers()["x-request-id"], "missing_case_4");
        let html = body_text(response).await;
        assert!(html.contains("<title>Missing &lt;page&gt;</title>"));
        assert!(html.contains("Cannot find &lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));

        let mut app = Application::with_config(config);
        app.get("/only-get", |_request| async { Ok(Response::text("ok")) })
            .expect("route registration");
        let request = AxumHttpRequest::builder()
            .method("POST")
            .uri("/only-get")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(
            body_text(response)
                .await
                .contains("This route is read-only.")
        );
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
            .header("x-request-id", "slow_req_17")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(response.headers()["x-request-id"], "slow_req_17");
    }

    #[tokio::test]
    async fn middleware_wraps_handlers_and_can_be_registered_after_routes() {
        let mut app = Application::new();
        app.get("/middleware", |_request| async {
            Ok(Response::text("wrapped"))
        })
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
        app.route_named(
            "users.show",
            HttpMethod::Get,
            "/users/{id}",
            |_request| async { Ok(Response::text("user")) },
        )
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
            app.route_named(
                "invalid name",
                HttpMethod::Get,
                "/bad-name",
                |_request| async { Ok(Response::text("bad")) }
            ),
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
            .write_all(
                b"GET /health HTTP/1.1\r\nHost: localhost\r\nx-request-id: tcp_health_1\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("write request");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read response");
        let response = String::from_utf8(bytes).expect("HTTP response is UTF-8");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(
            response
                .to_ascii_lowercase()
                .contains("x-request-id: tcp_health_1")
        );
        assert!(
            response
                .to_ascii_lowercase()
                .contains("content-type: text/plain; charset=utf-8")
        );
        assert!(response.ends_with("ok"));

        shutdown_tx.send(()).expect("shutdown signal");
        server
            .await
            .expect("server task")
            .expect("graceful server stop");
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

    #[tokio::test]
    async fn route_renders_shared_component_catalog_with_request_props_and_layout() {
        let article = SharedComponentDefinition::new("ArticlePage", |_context, _initial| {
            let render: ComponentRender = Rc::new(|properties, _children| {
                let id = properties
                    .iter()
                    .find(|property| property.name() == "id")
                    .and_then(|property| match property.value() {
                        PropertyValue::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "missing".to_owned());
                Ok(View::element(
                    "article",
                    [View::element("h1", [View::text(id)])?],
                )?)
            });
            Ok(render)
        })
        .expect("valid shared article definition")
        .server_only();

        let layout = SharedComponentDefinition::new("AppLayout", |_context, _properties| {
            let render: ComponentRender =
                Rc::new(|_properties, children| Ok(View::element("main", children.to_vec())?));
            Ok(render)
        })
        .expect("valid shared layout definition");

        let mut builder = SharedComponentCatalogBuilder::new();
        builder.register(article).expect("register article");
        builder.register(layout).expect("register layout");
        let catalog = Arc::new(builder.build());
        let route_catalog = Arc::clone(&catalog);

        let mut app = Application::new();
        app.get("/articles/{id}", move |request| {
            let catalog = Arc::clone(&route_catalog);
            async move {
                let id = request.path_param("id").unwrap_or("missing").to_owned();
                let property =
                    Property::new("id", id).map_err(|_| HttpError::internal_server_error())?;
                HtmlPage::from_component("Article & details", "ArticlePage", [property], [])?
                    .language("es")
                    .with_shared_components(&catalog)
                    .with_layout("AppLayout", [])?
                    .render()
            }
        })
        .expect("route registration");

        let request = AxumHttpRequest::builder()
            .uri("/articles/42")
            .body(Body::empty())
            .expect("request");
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        let html = body_text(response).await;
        assert!(html.contains("<html lang=\"es\">"));
        assert!(html.contains("<title>Article &amp; details</title>"));
        assert!(html.contains("<main><article><h1>42</h1></article></main>"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shared_catalog_is_reused_across_concurrent_http_requests_and_isolates_state() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SharedComponentCatalog>();

        let cleanup_count = Arc::new(AtomicUsize::new(0));
        let setup_cleanup_count = Arc::clone(&cleanup_count);
        let article = SharedComponentDefinition::new("ArticlePage", move |context, properties| {
            let id = properties
                .iter()
                .find(|property| property.name() == "id")
                .and_then(|property| match property.value() {
                    PropertyValue::Text(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "missing".to_owned());
            let state = signal(id);
            let cleanup_count = Arc::clone(&setup_cleanup_count);
            context.on_cleanup(move || {
                cleanup_count.fetch_add(1, Ordering::SeqCst);
            })?;
            let render: ComponentRender = Rc::new(move |_properties, _children| {
                Ok(View::element(
                    "article",
                    [View::element("h1", [View::text(state.get())])?],
                )?)
            });
            Ok(render)
        })
        .expect("shareable article definition")
        .server_only();

        let layout = SharedComponentDefinition::new("AppLayout", |_context, _properties| {
            let render: ComponentRender =
                Rc::new(|_properties, children| Ok(View::element("main", children.to_vec())?));
            Ok(render)
        })
        .expect("shareable layout definition");

        let mut builder = SharedComponentCatalogBuilder::new();
        builder.register(article).expect("register article");
        builder.register(layout).expect("register layout");
        let catalog = Arc::new(builder.build());

        let route_catalog = Arc::clone(&catalog);
        let mut app = Application::new();
        app.get("/articles/{id}", move |request| {
            let catalog = Arc::clone(&route_catalog);
            async move {
                let id = request.path_param("id").unwrap_or("missing").to_owned();
                let prop =
                    Property::new("id", id).map_err(|_| HttpError::internal_server_error())?;
                HtmlPage::from_component("Article", "ArticlePage", [prop], [])?
                    .language("es")
                    .with_shared_components(&catalog)
                    .with_layout("AppLayout", [])?
                    .render()
            }
        })
        .expect("route registration");

        let router = app.into_router(BindKind::Plain).expect("router");
        let make_request = |id: &str| {
            AxumHttpRequest::builder()
                .uri(format!("/articles/{id}"))
                .body(Body::empty())
                .expect("request")
        };
        let (first, second, repeated) = tokio::join!(
            router.clone().oneshot(make_request("7")),
            router.clone().oneshot(make_request("12")),
            router.clone().oneshot(make_request("7")),
        );

        let first_html = body_text(first.expect("first request")).await;
        let second_html = body_text(second.expect("second request")).await;
        let repeated_html = body_text(repeated.expect("repeated request")).await;
        assert!(first_html.contains("<main><article><h1>7</h1></article></main>"));
        assert!(second_html.contains("<main><article><h1>12</h1></article></main>"));
        assert!(repeated_html.contains("<main><article><h1>7</h1></article></main>"));
        assert_eq!(first_html, repeated_html);
        assert_eq!(
            cleanup_count.load(Ordering::SeqCst),
            3,
            "every request must dispose its local component scope"
        );
    }

    #[tokio::test]
    async fn route_groups_prefix_paths_and_names_and_scope_middleware() {
        let mut app = Application::new();
        app.get("/open", |_request| async { Ok(Response::text("open")) })
            .expect("open route");
        app.group("/api", |api| {
            api.name_prefix("api")?;
            api.middleware(Middleware::new(|request, next| async move {
                let response = next.run(request).await?;
                Ok(response.with_header(
                    http::HeaderName::from_static("x-group"),
                    http::HeaderValue::from_static("api"),
                ))
            }));
            api.route_named("health", HttpMethod::Get, "/health", |_request| async {
                Ok(Response::text("ok"))
            })?;
            api.group("/v1", |v1| {
                v1.get("/users/{id}", |request| async move {
                    Ok(Response::text(
                        request.path_param("id").unwrap_or("missing").to_owned(),
                    ))
                })?;
                Ok(())
            })?;
            Ok(())
        })
        .expect("api group");

        let routes = app.routes();
        assert!(routes.contains(&RouteInfo {
            method: HttpMethod::Get,
            path: "/api/health".to_owned(),
            name: Some("api.health".to_owned()),
        }));
        assert!(routes.contains(&RouteInfo {
            method: HttpMethod::Get,
            path: "/api/v1/users/{id}".to_owned(),
            name: None,
        }));

        let mut app = Application::new();
        app.group("/api", |api| {
            api.middleware(Middleware::new(|request, next| async move {
                let response = next.run(request).await?;
                Ok(response.with_header(
                    http::HeaderName::from_static("x-group"),
                    http::HeaderValue::from_static("api"),
                ))
            }));
            api.get("/health", |_request| async { Ok(Response::text("ok")) })?;
            Ok(())
        })
        .expect("group");
        app.get("/open", |_request| async { Ok(Response::text("open")) })
            .expect("open");
        let grouped = send(
            app,
            AxumHttpRequest::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(grouped.status(), StatusCode::OK);
        assert_eq!(grouped.headers()["x-group"], "api");
        assert_eq!(body_text(grouped).await, "ok");

        let mut app = Application::new();
        app.group("/api", |api| {
            api.middleware(Middleware::new(|request, next| async move {
                let response = next.run(request).await?;
                Ok(response.with_header(
                    http::HeaderName::from_static("x-group"),
                    http::HeaderValue::from_static("api"),
                ))
            }));
            api.get("/health", |_request| async { Ok(Response::text("ok")) })?;
            Ok(())
        })
        .expect("group");
        app.get("/open", |_request| async { Ok(Response::text("open")) })
            .expect("open");
        let open = send(
            app,
            AxumHttpRequest::builder()
                .uri("/open")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(open.status(), StatusCode::OK);
        assert!(open.headers().get("x-group").is_none());
        assert_eq!(body_text(open).await, "open");
    }

    #[tokio::test]
    async fn route_scoped_middleware_runs_after_global_middleware() {
        let mut app = Application::new();
        app.get("/scoped", |_request| async { Ok(Response::text("ok")) })
            .expect("route");
        app.middleware(Middleware::new(|request, next| async move {
            let response = next.run(request).await?;
            Ok(response.with_header(
                http::HeaderName::from_static("x-global"),
                http::HeaderValue::from_static("yes"),
            ))
        }))
        .expect("global");
        app.route_middleware(
            HttpMethod::Get,
            "/scoped",
            Middleware::new(|request, next| async move {
                let response = next.run(request).await?;
                Ok(response.with_header(
                    http::HeaderName::from_static("x-route"),
                    http::HeaderValue::from_static("yes"),
                ))
            }),
        )
        .expect("route middleware");

        let response = send(
            app,
            AxumHttpRequest::builder()
                .uri("/scoped")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.headers()["x-global"], "yes");
        assert_eq!(response.headers()["x-route"], "yes");
        assert_eq!(body_text(response).await, "ok");
    }

    #[test]
    fn production_config_requires_tls_or_trusted_proxies() {
        assert!(matches!(
            ServerConfig::production().validate(),
            Err(ServerError::ProductionHttpsRequiresTlsOrTrustedProxy)
        ));
        let proxied = ServerConfig::production()
            .with_trusted_proxies([IpNetwork::parse("10.0.0.0/8").expect("cidr")])
            .expect("proxies");
        proxied
            .validate()
            .expect("plain serve behind proxy is valid");
        ServerConfig::production()
            .with_direct_client_addresses()
            .validate_tls()
            .expect("direct production TLS bind is valid");
        assert_eq!(proxied.deployment(), DeploymentProfile::Production);
        assert!(proxied.require_https());
        assert!(matches!(
            proxied.client_address_policy(),
            ClientAddressPolicy::TrustedProxies(_)
        ));
        assert!(matches!(
            ServerConfig::new().with_trusted_proxies([] as [IpNetwork; 0]),
            Err(ServerError::EmptyTrustedProxyList)
        ));
        assert!(IpNetwork::parse("not-an-ip").is_err());
    }
}
