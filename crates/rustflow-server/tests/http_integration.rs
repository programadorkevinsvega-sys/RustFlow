use std::{net::SocketAddr, time::Duration};

use http::StatusCode;
use rustflow::view::View;
use rustflow_server::{
    Application, CsrfProtection, ErrorPages, HttpError, HttpMethod, IpNetwork, Middleware,
    Request, Response, ServerConfig, ServerError, ValidateInput, ValidationErrors, html_document,
    tls_server_config_from_pem,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};

async fn start_server(
    app: Application,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    JoinHandle<Result<(), ServerError>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral listener");
    let address = listener.local_addr().expect("listener address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(app.serve_listener_with_shutdown(listener, async move {
        let _ = shutdown_rx.await;
    }));
    (address, shutdown_tx, task)
}

async fn stop_server(shutdown: oneshot::Sender<()>, task: JoinHandle<Result<(), ServerError>>) {
    shutdown.send(()).expect("signal shutdown");
    task.await.expect("server task").expect("graceful shutdown");
}

async fn send_http(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address))
        .await
        .expect("timed out connecting to server")
        .expect("connect to server");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);

    tokio::time::timeout(Duration::from_secs(2), stream.write_all(request.as_bytes()))
        .await
        .expect("timed out writing HTTP request")
        .expect("write HTTP request");
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut bytes))
        .await
        .expect("timed out reading HTTP response")
        .expect("read HTTP response");
    String::from_utf8(bytes).expect("HTTP response is UTF-8")
}

fn status_code(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("valid HTTP status line")
}

fn response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("HTTP response headers")
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct UserQuery {
    detailed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    id: String,
    detailed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct EchoPayload {
    message: String,
}

#[tokio::test]
async fn real_tcp_routes_cover_safe_html_json_query_and_correlation_headers() {
    let mut app = Application::new();

    app.get("/api/health", |_request: Request| async {
        Response::json(&HealthResponse { status: "ok" })
    })
    .expect("health route");

    app.get("/users/{id}", |request: Request| async move {
        let query = request.query::<UserQuery>()?;
        Response::json(&UserResponse {
            id: request.path_param("id").unwrap_or("unknown").to_owned(),
            detailed: query.detailed.unwrap_or(false),
        })
    })
    .expect("user route");

    app.post("/api/echo", |request: Request| async move {
        let payload = request.json::<EchoPayload>()?;
        Response::json(&payload)
    })
    .expect("echo route");

    app.get("/", |_request: Request| async {
        let body = View::element("main", [View::text("<safe> & connected")])
            .map_err(|_| HttpError::internal_server_error())?;
        html_document("RustFlow <TCP>", "en", &body)
    })
    .expect("document route");

    let (address, shutdown, task) = start_server(app).await;

    let health = send_http(
        address,
        "GET",
        "/api/health",
        &[("x-request-id", "integration_health_1")],
        "",
    )
    .await;
    assert_eq!(status_code(&health), 200);
    assert!(
        health
            .to_ascii_lowercase()
            .contains("content-type: application/json; charset=utf-8")
    );
    assert!(
        health
            .to_ascii_lowercase()
            .contains("x-request-id: integration_health_1")
    );
    assert!(response_body(&health).contains(r#"{"status":"ok"}"#));

    let user = send_http(
        address,
        "GET",
        "/users/42?detailed=true",
        &[("x-request-id", "integration_user_2")],
        "",
    )
    .await;
    assert_eq!(status_code(&user), 200);
    assert!(response_body(&user).contains(r#"{"id":"42","detailed":true}"#));
    assert!(
        user.to_ascii_lowercase()
            .contains("x-request-id: integration_user_2")
    );

    let echo = send_http(
        address,
        "POST",
        "/api/echo",
        &[("content-type", "application/json")],
        r#"{"message":"hello"}"#,
    )
    .await;
    assert_eq!(status_code(&echo), 200);
    assert!(response_body(&echo).contains(r#"{"message":"hello"}"#));

    let page = send_http(address, "GET", "/", &[], "").await;
    assert_eq!(status_code(&page), 200);
    assert!(response_body(&page).starts_with("<!doctype html><html lang=\"en\">"));
    assert!(response_body(&page).contains("<title>RustFlow &lt;TCP&gt;</title>"));
    assert!(response_body(&page).contains("&lt;safe&gt; &amp; connected"));
    assert!(
        page.to_ascii_lowercase()
            .contains("content-type: text/html; charset=utf-8")
    );
    assert!(page.to_ascii_lowercase().contains("x-request-id: rf-"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_errors_cover_custom_404_405_and_invalid_query_400() {
    let error_pages = ErrorPages::new()
        .with_page(
            StatusCode::NOT_FOUND,
            "Not found <page>",
            "Missing <resource>.",
        )
        .expect("404 status")
        .with_page(
            StatusCode::METHOD_NOT_ALLOWED,
            "Wrong method",
            "This endpoint is read-only.",
        )
        .expect("405 status");
    let config = ServerConfig::new().with_error_pages(error_pages);
    let mut app = Application::with_config(config);

    app.get("/known", |_request| async {
        Ok(Response::text("available"))
    })
    .expect("known route");
    app.get("/users/{id}", |request: Request| async move {
        let query = request.query::<UserQuery>()?;
        Response::json(&UserResponse {
            id: request.path_param("id").unwrap_or("unknown").to_owned(),
            detailed: query.detailed.unwrap_or(false),
        })
    })
    .expect("user route");

    let (address, shutdown, task) = start_server(app).await;

    let missing = send_http(
        address,
        "GET",
        "/missing",
        &[("x-request-id", "network_404")],
        "",
    )
    .await;
    assert_eq!(status_code(&missing), 404);
    assert!(
        missing
            .to_ascii_lowercase()
            .contains("content-type: text/html; charset=utf-8")
    );
    assert!(
        missing
            .to_ascii_lowercase()
            .contains("x-request-id: network_404")
    );
    assert!(response_body(&missing).contains("<title>Not found &lt;page&gt;</title>"));
    assert!(response_body(&missing).contains("Missing &lt;resource&gt;."));
    assert!(!response_body(&missing).contains("<resource>"));

    let wrong_method = send_http(address, "POST", "/known", &[], "").await;
    assert_eq!(status_code(&wrong_method), 405);
    assert!(response_body(&wrong_method).contains("This endpoint is read-only."));

    let invalid_query = send_http(address, "GET", "/users/42?detailed=perhaps", &[], "").await;
    assert_eq!(status_code(&invalid_query), 400);
    assert!(response_body(&invalid_query).contains("Invalid query parameters"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_enforces_body_limits_and_handler_timeouts() {
    let config = ServerConfig::new()
        .with_max_body_bytes(4)
        .with_request_timeout(Duration::from_millis(100));
    let mut app = Application::with_config(config);

    app.post("/echo", |request: Request| async move {
        Ok(Response::new(StatusCode::OK, request.body().to_vec()))
    })
    .expect("echo route");
    app.get("/slow", |_request: Request| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(Response::text("too late"))
    })
    .expect("slow route");

    let (address, shutdown, task) = start_server(app).await;

    let oversized = send_http(
        address,
        "POST",
        "/echo",
        &[("x-request-id", "network_413")],
        "12345",
    )
    .await;
    assert_eq!(status_code(&oversized), 413);
    assert!(
        oversized
            .to_ascii_lowercase()
            .contains("x-request-id: network_413")
    );
    assert!(response_body(&oversized).contains("Request body is too large"));

    let timed_out = send_http(
        address,
        "GET",
        "/slow",
        &[("x-request-id", "network_504")],
        "",
    )
    .await;
    assert_eq!(status_code(&timed_out), 504);
    assert!(
        timed_out
            .to_ascii_lowercase()
            .contains("x-request-id: network_504")
    );
    assert!(response_body(&timed_out).contains("Request timed out"));

    stop_server(shutdown, task).await;
}

async fn send_raw(address: SocketAddr, bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(address))
        .await
        .expect("timed out connecting to server")?;
    tokio::time::timeout(Duration::from_secs(2), stream.write_all(bytes))
        .await
        .expect("timed out writing raw bytes")?;
    let _ = stream.shutdown().await;
    let mut response = Vec::new();
    match tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response)).await {
        Ok(Ok(_)) => Ok(response),
        Ok(Err(error)) => Err(error),
        Err(_) => Ok(response),
    }
}

#[tokio::test]
async fn real_tcp_route_groups_and_route_middleware_are_isolated() {
    let mut app = Application::new();
    app.get("/open", |_request: Request| async {
        Ok(Response::text("open"))
    })
    .expect("open route");
    app.group("/api", |api| {
        api.middleware(Middleware::new(|request, next| async move {
            let response = next.run(request).await?;
            Ok(response.with_header(
                http::HeaderName::from_static("x-group"),
                http::HeaderValue::from_static("api"),
            ))
        }));
        api.get("/health", |_request: Request| async {
            Ok(Response::text("ok"))
        })?;
        Ok(())
    })
    .expect("api group");
    app.route_middleware(
        HttpMethod::Get,
        "/open",
        Middleware::new(|request, next| async move {
            let response = next.run(request).await?;
            Ok(response.with_header(
                http::HeaderName::from_static("x-route"),
                http::HeaderValue::from_static("open"),
            ))
        }),
    )
    .expect("open middleware");

    let (address, shutdown, task) = start_server(app).await;

    let grouped = send_http(address, "GET", "/api/health", &[], "").await;
    assert_eq!(status_code(&grouped), 200);
    assert!(grouped.to_ascii_lowercase().contains("x-group: api"));
    assert!(!grouped.to_ascii_lowercase().contains("x-route: open"));

    let open = send_http(address, "GET", "/open", &[], "").await;
    assert_eq!(status_code(&open), 200);
    assert!(open.to_ascii_lowercase().contains("x-route: open"));
    assert!(!open.to_ascii_lowercase().contains("x-group: api"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_trusted_proxies_reconstruct_client_and_reject_spoofed_headers() {
    let trusted = ServerConfig::new()
        .with_trusted_proxies([IpNetwork::parse("127.0.0.1/32").expect("loopback")])
        .expect("trusted proxies");
    let mut trusted_app = Application::with_config(trusted);
    trusted_app
        .get("/who", |request: Request| async move {
            Ok(Response::text(format!(
                "{} {}",
                request
                    .client_ip()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                request.is_https()
            )))
        })
        .expect("who route");
    let (address, shutdown, task) = start_server(trusted_app).await;
    let trusted = send_http(
        address,
        "GET",
        "/who",
        &[
            ("x-forwarded-for", "203.0.113.8"),
            ("x-forwarded-proto", "https"),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&trusted), 200);
    assert_eq!(response_body(&trusted).trim_end(), "203.0.113.8 true");
    stop_server(shutdown, task).await;

    let untrusted = ServerConfig::new()
        .with_trusted_proxies([IpNetwork::parse("10.0.0.1/32").expect("other hop")])
        .expect("untrusted");
    let mut untrusted_app = Application::with_config(untrusted);
    untrusted_app
        .get("/who", |request: Request| async move {
            Ok(Response::text(format!(
                "{} {}",
                request
                    .client_ip()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                request.is_https()
            )))
        })
        .expect("who route");
    let (address, shutdown, task) = start_server(untrusted_app).await;
    let spoofed = send_http(
        address,
        "GET",
        "/who",
        &[
            ("x-forwarded-for", "203.0.113.8"),
            ("x-forwarded-proto", "https"),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&spoofed), 200);
    let body = response_body(&spoofed).trim_end();
    assert!(body.starts_with("127.0.0.1 "), "{body}");
    assert!(body.ends_with(" false"), "{body}");
    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_https_requirement_uses_trusted_proto_only() {
    let config = ServerConfig::production()
        .with_trusted_proxies([IpNetwork::parse("127.0.0.1/32").expect("loopback")])
        .expect("production proxy");
    let mut app = Application::with_config(config);
    app.get("/secure", |_request: Request| async {
        Ok(Response::text("ok"))
    })
    .expect("secure route");
    let (address, shutdown, task) = start_server(app).await;

    let denied = send_http(address, "GET", "/secure", &[], "").await;
    assert_eq!(status_code(&denied), 400);
    assert!(response_body(&denied).contains("HTTPS is required"));

    let allowed = send_http(
        address,
        "GET",
        "/secure",
        &[("x-forwarded-proto", "https")],
        "",
    )
    .await;
    assert_eq!(status_code(&allowed), 200);
    assert_eq!(response_body(&allowed).trim_end(), "ok");

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_survives_client_aborts_malformed_input_and_bounded_concurrency() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let config = ServerConfig::new().with_max_concurrent_requests(2);
    let mut app = Application::with_config(config);
    app.get("/health", |_request: Request| async {
        Ok(Response::text("ok"))
    })
    .expect("health");
    let in_flight_handler = in_flight.clone();
    let peak_handler = peak.clone();
    app.get("/hold", move |_request: Request| {
        let in_flight = in_flight_handler.clone();
        let peak = peak_handler.clone();
        async move {
            let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(80)).await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(Response::text("held"))
        }
    })
    .expect("hold");

    let (address, shutdown, task) = start_server(app).await;

    {
        let mut stream = TcpStream::connect(address).await.expect("abort connect");
        stream
            .write_all(
                b"POST /health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 32\r\n\r\npartial",
            )
            .await
            .expect("partial write");
        drop(stream);
    }

    let _ = send_raw(address, b"NOTHTTP\r\n\r\n").await;
    let _ = send_raw(address, b"GET / HTTP/1.1\r\nHost: \x00\r\n\r\n").await;

    let healthy = send_http(address, "GET", "/health", &[], "").await;
    assert_eq!(status_code(&healthy), 200);
    assert_eq!(response_body(&healthy).trim_end(), "ok");

    let mut joins = Vec::new();
    for _ in 0..6 {
        joins.push(tokio::spawn(send_http(address, "GET", "/hold", &[], "")));
    }
    for join in joins {
        let held = join.await.expect("join hold request");
        assert_eq!(status_code(&held), 200);
        assert_eq!(response_body(&held).trim_end(), "held");
    }
    assert!(peak.load(Ordering::SeqCst) <= 2);

    stop_server(shutdown, task).await;
}

fn test_tls_material() -> (Vec<u8>, Vec<u8>, rustls::RootCertStore) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certified = rcgen::generate_simple_self_signed(["localhost".to_owned()])
        .expect("self-signed certificate");
    let cert_pem = certified.cert.pem().into_bytes();
    let key_pem = certified.signing_key.serialize_pem().into_bytes();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(certified.cert.der().clone())
        .expect("trust test certificate");
    (cert_pem, key_pem, roots)
}

#[tokio::test]
async fn incomplete_tls_handshake_does_not_block_valid_connections() {
    let (cert_pem, key_pem, roots) = test_tls_material();
    let tls = tls_server_config_from_pem(&cert_pem, &key_pem).expect("tls config");
    let mut app = Application::new();
    app.get("/health", |request: Request| async move {
        Ok(Response::text(if request.is_https() {
            "secure"
        } else {
            "plain"
        }))
    })
    .expect("health");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tls listener");
    let address = listener.local_addr().expect("tls address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(app.serve_tls_with_shutdown(listener, tls, async move {
        let _ = shutdown_rx.await;
    }));

    // Connect without sending a ClientHello. This socket must not serialize
    // TLS acceptance and delay the following valid client.
    let stalled = TcpStream::connect(address)
        .await
        .expect("stalled tcp connect");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let tcp = TcpStream::connect(address)
        .await
        .expect("valid tcp connect");
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ));
    let server_name = rustls_pki_types::ServerName::try_from("localhost").expect("dns name");
    let mut stream =
        tokio::time::timeout(Duration::from_secs(2), connector.connect(server_name, tcp))
            .await
            .expect("a stalled peer must not block a valid TLS handshake")
            .expect("valid tls handshake");

    tokio::time::timeout(
        Duration::from_secs(2),
        stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
    )
    .await
    .expect("write tls request timeout")
    .expect("write tls request");

    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut bytes))
        .await
        .expect("read tls response timeout")
        .expect("read tls response");
    let response = String::from_utf8(bytes).expect("utf8 response");
    assert_eq!(status_code(&response), 200);
    assert!(response_body(&response).contains("secure"));

    drop(stalled);
    shutdown_tx.send(()).expect("tls shutdown");
    task.await.expect("tls task").expect("tls server stop");
}

#[tokio::test]
async fn real_tls_serves_https_and_marks_the_connection_secure() {
    let (cert_pem, key_pem, roots) = test_tls_material();
    let tls = tls_server_config_from_pem(&cert_pem, &key_pem).expect("tls config");
    let mut app = Application::new();
    app.get("/health", |request: Request| async move {
        Ok(Response::text(if request.is_https() {
            "secure"
        } else {
            "plain"
        }))
    })
    .expect("health");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tls listener");
    let address = listener.local_addr().expect("tls address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(app.serve_tls_with_shutdown(listener, tls, async move {
        let _ = shutdown_rx.await;
    }));

    let tcp = TcpStream::connect(address).await.expect("tls tcp");
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ));
    let server_name = rustls_pki_types::ServerName::try_from("localhost").expect("dns name");
    let mut stream = connector
        .connect(server_name, tcp)
        .await
        .expect("tls handshake");
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write tls request");
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .expect("read tls response");
    let response = String::from_utf8(bytes).expect("utf8");
    assert_eq!(status_code(&response), 200);
    assert!(response_body(&response).contains("secure"));

    shutdown_tx.send(()).expect("tls shutdown");
    task.await.expect("tls task").expect("tls server stop");
}


#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignupForm {
    email: String,
    age: u8,
}

impl ValidateInput for SignupForm {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if !self.email.contains('@') {
            errors.add_field_error("email", "Enter a valid email address.");
        }
        if self.age < 18 {
            errors.add_field_error("age", "You must be at least 18 years old.");
        }
        if !errors.is_empty() {
            errors.add_form_error("Please correct the highlighted fields.");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[tokio::test]
async fn real_tcp_typed_forms_validate_and_return_structured_errors() {
    let mut app = Application::new();
    app.post("/signup", |request: Request| async move {
        let signup = request.validated_form::<SignupForm>()?;
        Ok(Response::text(format!("accepted:{}", signup.email)))
    })
    .expect("signup route");
    app.post("/signup-json", |request: Request| async move {
        let signup = request.validated_json::<SignupForm>()?;
        Ok(Response::text(format!("accepted:{}", signup.email)))
    })
    .expect("signup json route");
    app.get("/signup-query", |request: Request| async move {
        let signup = request.validated_query::<SignupForm>()?;
        Ok(Response::text(format!("accepted:{}", signup.email)))
    })
    .expect("signup query route");
    let (address, shutdown, task) = start_server(app).await;

    let valid = send_http(
        address,
        "POST",
        "/signup",
        &[("content-type", "application/x-www-form-urlencoded")],
        "email=person%40example.test&age=21",
    )
    .await;
    assert_eq!(status_code(&valid), 200);
    assert!(response_body(&valid).contains("accepted:person@example.test"));

    let invalid = send_http(
        address,
        "POST",
        "/signup",
        &[("content-type", "application/x-www-form-urlencoded")],
        "email=bad&age=16",
    )
    .await;
    assert_eq!(status_code(&invalid), 422);
    let payload = response_body(&invalid);
    assert!(payload.contains("\"error\":\"validation_failed\""));
    assert!(payload.contains("\"fieldErrors\""));
    assert!(payload.contains("\"email\""));
    assert!(payload.contains("\"age\""));
    assert!(!payload.contains("bad"), "raw user input must not be echoed");

    let unexpected = send_http(
        address,
        "POST",
        "/signup",
        &[("content-type", "application/x-www-form-urlencoded")],
        "email=person%40example.test&age=21&admin=true",
    )
    .await;
    assert_eq!(status_code(&unexpected), 400);
    assert!(response_body(&unexpected).contains("Invalid form input"));

    let wrong_content_type = send_http(
        address,
        "POST",
        "/signup",
        &[("content-type", "application/json")],
        "{\"email\":\"person@example.test\",\"age\":21}",
    )
    .await;
    assert_eq!(status_code(&wrong_content_type), 415);

    let json = send_http(
        address,
        "POST",
        "/signup-json",
        &[("content-type", "application/json")],
        r#"{"email":"json@example.test","age":29}"#,
    )
    .await;
    assert_eq!(status_code(&json), 200);
    assert!(response_body(&json).contains("accepted:json@example.test"));

    let query = send_http(
        address,
        "GET",
        "/signup-query?email=query%40example.test&age=24",
        &[],
        "",
    )
    .await;
    assert_eq!(status_code(&query), 200);
    assert!(response_body(&query).contains("accepted:query@example.test"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_rejects_oversized_payloads_before_handler_execution() {
    let mut app = Application::with_config(ServerConfig::new().with_max_body_bytes(8));
    app.post("/submit", |_request: Request| async {
        Ok(Response::text("must not run"))
    })
    .expect("submit route");
    let (address, shutdown, task) = start_server(app).await;

    let response = send_http(
        address,
        "POST",
        "/submit",
        &[("content-type", "application/x-www-form-urlencoded")],
        "payload=too-large",
    )
    .await;
    assert_eq!(status_code(&response), 413);
    assert!(response_body(&response).contains("Request body is too large"));
    assert!(!response_body(&response).contains("must not run"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_csrf_middleware_requires_a_valid_cookie_and_matching_token() {
    let csrf = CsrfProtection::new([9_u8; 32]).expect("strong application key");
    let mut app = Application::new();
    app.middleware(csrf.middleware())
        .expect("register CSRF middleware");
    let token_protection = csrf.clone();
    app.get("/token", move |request: Request| {
        let protection = token_protection.clone();
        async move {
            let token = protection
                .token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = protection
                .cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text(token.as_str().to_owned())
                .with_header(http::header::SET_COOKIE, cookie))
        }
    })
    .expect("token route");
    app.get("/public", |_request: Request| async {
        Ok(Response::text("public"))
    })
    .expect("public route");
    app.post("/mutate", |_request: Request| async {
        Ok(Response::text("mutated"))
    })
    .expect("mutation route");
    let (address, shutdown, task) = start_server(app).await;

    let public = send_http(address, "GET", "/public", &[], "").await;
    assert_eq!(status_code(&public), 200);

    let token_response = send_http(address, "GET", "/token", &[], "").await;
    assert_eq!(status_code(&token_response), 200);
    let token = response_body(&token_response).trim().to_owned();
    assert!(!token.is_empty());
    let lower_headers = token_response.to_ascii_lowercase();
    assert!(lower_headers.contains("set-cookie: rustflow_csrf="));
    assert!(lower_headers.contains("samesite=lax; httponly"));

    let rejected_without_token = send_http(address, "POST", "/mutate", &[], "").await;
    assert_eq!(status_code(&rejected_without_token), 403);

    let cookie = format!("rustflow_csrf={token}");
    let rejected_mismatch = send_http(
        address,
        "POST",
        "/mutate",
        &[
            ("cookie", cookie.as_str()),
            ("x-csrf-token", "different-token"),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&rejected_mismatch), 403);

    let accepted = send_http(
        address,
        "POST",
        "/mutate",
        &[
            ("cookie", cookie.as_str()),
            ("x-csrf-token", token.as_str()),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&accepted), 200);
    assert!(response_body(&accepted).contains("mutated"));

    let form_body = format!("_csrf={token}&comment=hello");
    let accepted_form_token = send_http(
        address,
        "POST",
        "/mutate",
        &[
            ("cookie", cookie.as_str()),
            ("content-type", "application/x-www-form-urlencoded"),
        ],
        &form_body,
    )
    .await;
    assert_eq!(status_code(&accepted_form_token), 200);
    assert!(response_body(&accepted_form_token).contains("mutated"));

    stop_server(shutdown, task).await;
}

#[tokio::test]
async fn real_tcp_redirect_back_accepts_same_origin_and_falls_back_for_external_referers() {
    let mut app = Application::new();
    app.post("/return", |request: Request| async move {
        request.redirect_back("/form")
    })
    .expect("return route");
    app.get("/bad-fallback", |request: Request| async move {
        request.redirect_back("//attacker.invalid")
    })
    .expect("bad fallback route");
    let (address, shutdown, task) = start_server(app).await;

    let same_origin = send_http(
        address,
        "POST",
        "/return",
        &[("referer", "http://localhost/previous?tab=1")],
        "",
    )
    .await;
    assert_eq!(status_code(&same_origin), 303);
    assert!(same_origin.to_ascii_lowercase().contains("location: /previous?tab=1"));

    let external = send_http(
        address,
        "POST",
        "/return",
        &[("referer", "https://attacker.invalid/phish")],
        "",
    )
    .await;
    assert_eq!(status_code(&external), 303);
    assert!(external.to_ascii_lowercase().contains("location: /form"));

    let unsafe_fallback = send_http(address, "GET", "/bad-fallback", &[], "").await;
    assert_eq!(status_code(&unsafe_fallback), 400);

    stop_server(shutdown, task).await;
}
