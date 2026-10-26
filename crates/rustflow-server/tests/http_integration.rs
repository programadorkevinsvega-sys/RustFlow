use std::{net::SocketAddr, time::Duration};

use http::StatusCode;
use rustflow::view::View;
use rustflow_server::{
    Application, ErrorPages, HttpError, Request, Response, ServerConfig, ServerError, html_document,
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
