use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use http::StatusCode;
use rustflow_server::{
    AccessPolicy, Application, AuthIdentity, CsrfProtection, HttpError, HttpMethod, Request,
    Response, RolePolicy, ServerError, SessionCookieConfig, SessionManager,
};
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
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(app.serve_listener_with_shutdown(listener, async move {
        let _ = shutdown_rx.await;
    }));
    (address, shutdown_tx, task)
}
async fn stop_server(shutdown: oneshot::Sender<()>, task: JoinHandle<Result<(), ServerError>>) {
    shutdown.send(()).expect("shutdown");
    task.await.expect("task").expect("server stop");
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
        .expect("connect timeout")
        .expect("connect");
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
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut bytes))
        .await
        .expect("read timeout")
        .expect("read");
    String::from_utf8(bytes).expect("utf8")
}
fn status_code(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("status")
}
fn response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("body")
}
fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
}
fn cookie_pair(response: &str, name: &str) -> String {
    let pair = header(response, "set-cookie")
        .expect("Set-Cookie")
        .split(';')
        .next()
        .expect("pair")
        .to_owned();
    assert!(pair.starts_with(&format!("{name}=")));
    pair
}

#[tokio::test]
async fn tcp_sessions_rotate_guards_authorize_and_preserve_csrf() {
    let sessions = SessionManager::in_memory(Duration::from_secs(300))
        .expect("manager")
        .with_cookie_config(SessionCookieConfig::development())
        .expect("development cookie");
    let csrf = CsrfProtection::new([0x42_u8; 32]).expect("CSRF");
    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new()
            .allow_role("admin.read", "admin")
            .expect("policy"),
    );
    let protected_called = Arc::new(AtomicBool::new(false));
    let protected_flag = protected_called.clone();
    let admin_called = Arc::new(AtomicBool::new(false));
    let admin_flag = admin_called.clone();

    let mut app = Application::new();
    app.middleware(sessions.middleware())
        .expect("session middleware");

    app.get("/session/start", |request: Request| async move {
        request
            .session()
            .ok_or_else(HttpError::internal_server_error)?
            .create()
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Response::text("anonymous session created"))
    })
    .expect("create anonymous session");

    let csrf_route = csrf.clone();
    app.get("/csrf", move |request: Request| {
        let csrf = csrf_route.clone();
        async move {
            let token = csrf
                .token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf
                .cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text(token.as_str().to_owned())
                .with_header(http::header::SET_COOKIE, cookie)
                .with_header(
                    http::header::CACHE_CONTROL,
                    http::HeaderValue::from_static("no-store"),
                ))
        }
    })
    .expect("csrf route");

    app.post("/login", |request: Request| async move {
        let session = request
            .session()
            .ok_or_else(HttpError::internal_server_error)?;
        // Credentials are deliberately represented by a trusted test fixture. Production code
        // must verify user credentials before assigning an identity.
        let identity = AuthIdentity::new("alice")
            .map_err(|_| HttpError::internal_server_error())?
            .with_role("reader")
            .map_err(|_| HttpError::internal_server_error())?;
        session
            .authenticate(identity)
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Response::text("signed in"))
    })
    .expect("login");
    app.route_middleware(HttpMethod::Post, "/login", csrf.clone().middleware())
        .expect("login CSRF");

    app.route_protected(HttpMethod::Get, "/private", move |request: Request| {
        let called = protected_flag.clone();
        async move {
            called.store(true, Ordering::SeqCst);
            let identity = request
                .identity()
                .map_err(|_| HttpError::internal_server_error())?
                .ok_or_else(|| {
                    HttpError::new(StatusCode::UNAUTHORIZED, "Authentication required")
                })?;
            Ok(Response::text(identity.subject().to_owned()))
        }
    })
    .expect("protected route");

    app.route_authorized(
        HttpMethod::Get,
        "/admin",
        "admin.read",
        Some(policy),
        move |_request: Request| {
            let called = admin_flag.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok(Response::text("admin"))
            }
        },
    )
    .expect("authorized route");

    app.post("/logout", |request: Request| async move {
        request
            .session()
            .ok_or_else(HttpError::internal_server_error)?
            .invalidate()
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Response::text("signed out"))
    })
    .expect("logout");
    app.route_middleware(HttpMethod::Post, "/logout", csrf.middleware())
        .expect("logout CSRF");

    let (address, shutdown, task) = start_server(app).await;
    let anonymous = send_http(address, "GET", "/private", &[], "").await;
    assert_eq!(status_code(&anonymous), 401);
    assert!(
        !protected_called.load(Ordering::SeqCst),
        "guard must stop before handler"
    );

    let initial = send_http(address, "GET", "/session/start", &[], "").await;
    assert_eq!(status_code(&initial), 200);
    let pre_auth_cookie = cookie_pair(&initial, "rustflow_session");

    let csrf_response = send_http(address, "GET", "/csrf", &[], "").await;
    assert_eq!(status_code(&csrf_response), 200);
    let csrf_token = response_body(&csrf_response).to_owned();
    let csrf_cookie = cookie_pair(&csrf_response, "rustflow_csrf");

    let login_cookie = format!("{csrf_cookie}; {pre_auth_cookie}");
    let login = send_http(
        address,
        "POST",
        "/login",
        &[
            ("cookie", &login_cookie),
            ("origin", "http://localhost"),
            ("x-csrf-token", &csrf_token),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&login), 200);
    let session_cookie = cookie_pair(&login, "rustflow_session");
    assert_ne!(
        session_cookie, pre_auth_cookie,
        "login must rotate the pre-authentication ID"
    );
    let session_set_cookie = header(&login, "set-cookie")
        .expect("session cookie")
        .to_ascii_lowercase();
    assert!(session_set_cookie.contains("httponly"));
    assert!(session_set_cookie.contains("samesite=lax"));
    assert!(session_set_cookie.contains("max-age="));

    let authenticated = send_http(
        address,
        "GET",
        "/private",
        &[("cookie", &session_cookie)],
        "",
    )
    .await;
    assert_eq!(status_code(&authenticated), 200);
    assert!(response_body(&authenticated).contains("alice"));

    let fixation_replay = send_http(
        address,
        "GET",
        "/private",
        &[("cookie", &pre_auth_cookie)],
        "",
    )
    .await;
    assert_eq!(
        status_code(&fixation_replay),
        401,
        "old pre-auth session must no longer authenticate"
    );

    let denied = send_http(address, "GET", "/admin", &[("cookie", &session_cookie)], "").await;
    assert_eq!(status_code(&denied), 403);
    assert!(
        !admin_called.load(Ordering::SeqCst),
        "denied route must not run handler"
    );

    let logout = send_http(
        address,
        "POST",
        "/logout",
        &[
            ("cookie", &format!("{csrf_cookie}; {session_cookie}")),
            ("origin", "http://localhost"),
            ("x-csrf-token", &csrf_token),
        ],
        "",
    )
    .await;
    assert_eq!(status_code(&logout), 200);
    assert!(
        header(&logout, "set-cookie")
            .expect("expired cookie")
            .to_ascii_lowercase()
            .contains("max-age=0")
    );
    let replay = send_http(
        address,
        "GET",
        "/private",
        &[("cookie", &session_cookie)],
        "",
    )
    .await;
    assert_eq!(status_code(&replay), 401);

    let bad_csrf = send_http(address, "POST", "/logout", &[("cookie", &csrf_cookie)], "").await;
    assert_eq!(status_code(&bad_csrf), 403);
    stop_server(shutdown, task).await;
}
