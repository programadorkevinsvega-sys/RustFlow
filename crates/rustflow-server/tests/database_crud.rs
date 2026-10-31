use std::{net::SocketAddr, sync::Arc, time::Duration};

use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, SET_COOKIE},
};
use rustflow::view::View;
use rustflow_server::{
    AccessPolicy, Application, AuthIdentity, CsrfProtection, DatabaseConfig, DatabasePool,
    HttpError, HttpMethod, PasswordHash, Request, Response, RolePolicy, ServerError,
    SessionCookieConfig, SessionManager, ValidateInput, ValidationErrors, html_document,
};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};

#[derive(Debug, Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

impl ValidateInput for LoginForm {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.username.trim().is_empty() {
            errors.add_field_error("username", "Enter a username.");
        }
        if self.password.is_empty() {
            errors.add_field_error("password", "Enter a password.");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct NewNoteForm {
    title: String,
}

impl ValidateInput for NewNoteForm {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        let title = self.title.trim();
        if title.is_empty() {
            errors.add_field_error("title", "A title is required.");
        } else if title.len() > 120 {
            errors.add_field_error("title", "The title must be at most 120 bytes.");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

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
    let mut stream = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(address))
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
    tokio::time::timeout(Duration::from_secs(3), stream.write_all(request.as_bytes()))
        .await
        .expect("write timeout")
        .expect("write");
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut bytes))
        .await
        .expect("read timeout")
        .expect("read");
    String::from_utf8(bytes).expect("HTTP response is UTF-8")
}

fn status(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .expect("status")
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
    let prefix = format!("{name}=");
    response
        .lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("set-cookie")
                .then_some(value.trim())
        })
        .find(|value| value.starts_with(&prefix))
        .expect("expected Set-Cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned()
}

fn response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("response body")
}

#[tokio::test]
async fn validated_crud_uses_sqlite_sessions_auth_csrf_and_server_rendering() {
    let database_config = DatabaseConfig::new("sqlite::memory:")
        .expect("SQLite URL")
        .with_max_connections(1)
        .expect("single connection");
    let database = Arc::new(DatabasePool::connect(&database_config).await.expect("pool"));
    let migrations = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sqlite_migrations");
    database
        .run_migrations(&migrations)
        .await
        .expect("apply migrations");

    let sessions = SessionManager::in_memory(Duration::from_secs(300))
        .expect("session manager")
        .with_cookie_config(SessionCookieConfig::development())
        .expect("development cookie");
    let csrf = CsrfProtection::new([0x39_u8; 32]).expect("test CSRF key");
    let stored_password =
        Arc::new(PasswordHash::hash("correct horse battery staple").expect("hash"));
    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new()
            .allow_role("notes.write", "writer")
            .expect("access rule"),
    );
    let mut app = Application::new();
    app.middleware(sessions.middleware())
        .expect("session middleware");

    let csrf_for_route = csrf.clone();
    app.get("/csrf", move |request: Request| {
        let csrf = csrf_for_route.clone();
        async move {
            let token = csrf
                .token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf
                .cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text(token.as_str().to_owned())
                .with_header(SET_COOKIE, cookie)
                .with_header(CACHE_CONTROL, HeaderValue::from_static("no-store")))
        }
    })
    .expect("csrf route");

    let stored_password_for_login = stored_password.clone();
    app.post("/login", move |request: Request| {
        let password_hash = stored_password_for_login.clone();
        async move {
            let form = request.validated_form::<LoginForm>()?;
            let accepted = form.username == "ada"
                && password_hash
                    .verify(&form.password)
                    .map_err(|_| HttpError::internal_server_error())?;
            if !accepted {
                return Err(HttpError::new(
                    StatusCode::UNAUTHORIZED,
                    "Invalid credentials",
                ));
            }
            let identity = AuthIdentity::new("user-ada")
                .map_err(|_| HttpError::internal_server_error())?
                .with_role("writer")
                .map_err(|_| HttpError::internal_server_error())?;
            request
                .session()
                .ok_or_else(HttpError::internal_server_error)?
                .authenticate(identity)
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text("signed in"))
        }
    })
    .expect("login route");
    app.route_middleware(HttpMethod::Post, "/login", csrf.clone().middleware())
        .expect("login CSRF");

    let database_for_create = database.clone();
    app.route_authorized(
        HttpMethod::Post,
        "/notes",
        "notes.write",
        Some(policy),
        move |request: Request| {
            let database = database_for_create.clone();
            async move {
                let input = request.validated_form::<NewNoteForm>()?;
                sqlx_insert_note(&database, input.title.trim()).await?;
                Ok(Response::new(StatusCode::SEE_OTHER, "saved")
                    .with_header(http::header::LOCATION, HeaderValue::from_static("/notes")))
            }
        },
    )
    .expect("create route");
    app.route_middleware(HttpMethod::Post, "/notes", csrf.clone().middleware())
        .expect("notes CSRF");

    let database_for_list = database.clone();
    app.route_protected(HttpMethod::Get, "/notes", move |_request: Request| {
        let database = database_for_list.clone();
        async move {
            let rows: Vec<String> = rustflow_server::sqlx::query_scalar(
                "SELECT name FROM rustflow_audit_items ORDER BY id",
            )
            .fetch_all(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .map_err(|error| rustflow_server::DatabaseError::from(error).to_http_error())?;
            let mut children = vec![
                View::element("h1", [View::text("Notes")])
                    .map_err(|_| HttpError::internal_server_error())?,
            ];
            let note_items = rows
                .into_iter()
                .map(|title| {
                    View::element("li", [View::text(title)])
                        .map_err(|_| HttpError::internal_server_error())
                })
                .collect::<Result<Vec<_>, _>>()?;
            children.push(
                View::element("ul", note_items).map_err(|_| HttpError::internal_server_error())?,
            );
            let body =
                View::element("main", children).map_err(|_| HttpError::internal_server_error())?;
            html_document("Notes", "en", &body)
        }
    })
    .expect("list route");

    let (address, shutdown, task) = start_server(app).await;
    let csrf_response = send_http(address, "GET", "/csrf", &[], "").await;
    assert_eq!(status(&csrf_response), 200);
    assert_eq!(header(&csrf_response, "cache-control"), Some("no-store"));
    let token = response_body(&csrf_response).to_owned();
    let csrf_cookie = cookie_pair(&csrf_response, "rustflow_csrf");

    let anonymous_list = send_http(address, "GET", "/notes", &[], "").await;
    assert_eq!(status(&anonymous_list), 401);

    let login = send_http(
        address,
        "POST",
        "/login",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "http://localhost"),
            ("cookie", &csrf_cookie),
            ("x-csrf-token", &token),
        ],
        "username=ada&password=incorrect",
    )
    .await;
    assert_eq!(
        status(&login),
        401,
        "wrong password must not create an identity"
    );

    let login = send_http(
        address,
        "POST",
        "/login",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "http://localhost"),
            ("cookie", &csrf_cookie),
            ("x-csrf-token", &token),
        ],
        "username=ada&password=correct%20horse%20battery%20staple",
    )
    .await;
    assert_eq!(status(&login), 200);
    let session_cookie = cookie_pair(&login, "rustflow_session");
    let cookies = format!("{csrf_cookie}; {session_cookie}");

    let invalid = send_http(
        address,
        "POST",
        "/notes",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "http://localhost"),
            ("cookie", &cookies),
            ("x-csrf-token", &token),
        ],
        "title=%20%20%20",
    )
    .await;
    assert_eq!(
        status(&invalid),
        422,
        "invalid fields must not be persisted"
    );

    let create = send_http(
        address,
        "POST",
        "/notes",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "http://localhost"),
            ("cookie", &cookies),
            ("x-csrf-token", &token),
        ],
        "title=Ledger%20%3Cscript%3E",
    )
    .await;
    assert_eq!(status(&create), 303);

    let list = send_http(address, "GET", "/notes", &[("cookie", &cookies)], "").await;
    assert_eq!(status(&list), 200);
    assert!(response_body(&list).contains("Ledger &lt;script&gt;"));
    assert!(!response_body(&list).contains("<script>"));
    let count: i64 =
        rustflow_server::sqlx::query_scalar("SELECT COUNT(*) FROM rustflow_audit_items")
            .fetch_one(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .expect("count");
    assert_eq!(
        count, 1,
        "only the validated successful request was persisted"
    );

    stop_server(shutdown, task).await;
}

async fn sqlx_insert_note(database: &DatabasePool, title: &str) -> Result<(), HttpError> {
    rustflow_server::sqlx::query("INSERT INTO rustflow_audit_items (name) VALUES (?)")
        .bind(title)
        .execute(database.sqlx_sqlite_pool().expect("SQLite pool"))
        .await
        .map_err(|error| rustflow_server::DatabaseError::from(error).to_http_error())?;
    Ok(())
}
