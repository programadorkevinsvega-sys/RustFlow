use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
const REPOSITORY_URL: &str = "https://github.com/programadorkevinsvega-sys/RustFlow.git";

const GENERATED_CRUD_MAIN: &str = r#"use std::{error::Error, sync::Arc, time::Duration};

use http::{header::{CACHE_CONTROL, SET_COOKIE}, HeaderValue, StatusCode};
use rustflow::view::View;
use rustflow_server::{
    AccessPolicy, AppConfig, AppEnvironment, Application, AuthIdentity, CsrfProtection,
    DatabaseConfig, DatabaseError, DatabasePool, HttpError, HttpMethod, PasswordHash, Request,
    Response, RolePolicy, SessionCookieConfig, SessionManager, ValidateInput, ValidationErrors,
    html_document,
};
use serde::Deserialize;

pub mod components;
pub mod controllers;
pub mod models;
pub mod pages;

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
        if errors.is_empty() { Ok(()) } else { Err(errors) }
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
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load()?;
    let address = config.socket_addr();
    let database_config = DatabaseConfig::from_app_config(&config)?;
    let database = Arc::new(DatabasePool::connect(&database_config).await?);
    if config.environment != AppEnvironment::Production {
        database.run_migrations("migrations").await?;
    }

    let sessions = SessionManager::in_memory(Duration::from_secs(300))
        .expect("session manager")
        .with_cookie_config(SessionCookieConfig::development())
        .expect("development cookie");
    let csrf = CsrfProtection::new([0x39_u8; 32]).expect("CSRF protection");
    let stored_password = Arc::new(
        PasswordHash::hash("correct horse battery staple").expect("hash test credential"),
    );
    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new()
            .allow_role("notes.write", "writer")
            .expect("access rule"),
    );

    let mut app = Application::new();
    app.middleware(sessions.middleware())?;

    let csrf_for_route = csrf.clone();
    app.get("/csrf", move |request: Request| {
        let csrf = csrf_for_route.clone();
        async move {
            let token = csrf.token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf.cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text(token.as_str().to_owned())
                .with_header(SET_COOKIE, cookie)
                .with_header(CACHE_CONTROL, HeaderValue::from_static("no-store")))
        }
    })?;

    let password_for_login = stored_password.clone();
    app.post("/login", move |request: Request| {
        let password_hash = password_for_login.clone();
        async move {
            let form = request.validated_form::<LoginForm>()?;
            let accepted = form.username == "ada"
                && password_hash.verify(&form.password)
                    .map_err(|_| HttpError::internal_server_error())?;
            if !accepted {
                return Err(HttpError::new(StatusCode::UNAUTHORIZED, "Invalid credentials"));
            }
            let identity = AuthIdentity::new("user-ada")
                .map_err(|_| HttpError::internal_server_error())?
                .with_role("writer")
                .map_err(|_| HttpError::internal_server_error())?;
            request.session().ok_or_else(HttpError::internal_server_error)?
                .authenticate(identity)
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::text("signed in"))
        }
    })?;
    app.route_middleware(HttpMethod::Post, "/login", csrf.clone().middleware())?;

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
                rustflow_server::sqlx::query("INSERT INTO notes (title) VALUES (?)")
                    .bind(input.title.trim())
                    .execute(database.sqlx_sqlite_pool().expect("SQLite pool"))
                    .await
                    .map_err(|error| DatabaseError::from(error).to_http_error())?;
                Ok(Response::new(StatusCode::SEE_OTHER, "saved")
                    .with_header(http::header::LOCATION, HeaderValue::from_static("/notes")))
            }
        },
    )?;
    app.route_middleware(HttpMethod::Post, "/notes", csrf.clone().middleware())?;

    let database_for_list = database.clone();
    app.route_protected(HttpMethod::Get, "/notes", move |_request: Request| {
        let database = database_for_list.clone();
        async move {
            let rows: Vec<String> = rustflow_server::sqlx::query_scalar(
                "SELECT title FROM notes ORDER BY id"
            )
            .fetch_all(database.sqlx_sqlite_pool().expect("SQLite pool"))
            .await
            .map_err(|error| DatabaseError::from(error).to_http_error())?;
            let mut children = vec![View::element("h1", [View::text("Notes")])
                .map_err(|_| HttpError::internal_server_error())?];
            let items = rows.into_iter().map(|title| {
                View::element("li", [View::text(title)])
                    .map_err(|_| HttpError::internal_server_error())
            }).collect::<Result<Vec<_>, _>>()?;
            children.push(View::element("ul", items)
                .map_err(|_| HttpError::internal_server_error())?);
            let body = View::element("main", children)
                .map_err(|_| HttpError::internal_server_error())?;
            html_document("Notes", "en", &body)
        }
    })?;

    let database_for_health = database.clone();
    app.get("/health/database", move |_request: Request| {
        let database = database_for_health.clone();
        async move {
            rustflow_server::sqlx::query("SELECT 1")
                .execute(database.sqlx_sqlite_pool().expect("SQLite pool"))
                .await
                .map_err(|error| DatabaseError::from(error).to_http_error())?;
            Ok(Response::text("ok"))
        }
    })?;
    app.get("/", controllers::home_controller::index)?;
    app.serve(address).await?;
    Ok(())
}
"#;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = env::temp_dir().join(format!(
            "rustflow-cli-it-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create isolated temporary directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rustflow"))
}

fn invoke(cwd: &Path, args: &[&str]) -> Output {
    cli()
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run rustflow CLI")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_project(parent: &Path, name: &str) -> PathBuf {
    let output = invoke(parent, &["new", name]);
    assert_success(&output);
    parent.join(name)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate lives under crates/rustflow-cli")
        .to_path_buf()
}

fn localize_framework_dependencies(project: &Path) {
    let root = workspace_root();
    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let rustflow_line = format!(
        "rustflow = {{ git = \"{REPOSITORY_URL}\", rev = \"{}\" }}",
        env!("RUSTFLOW_GIT_REV")
    );
    let server_line = format!(
        "rustflow-server = {{ git = \"{REPOSITORY_URL}\", rev = \"{}\" }}",
        env!("RUSTFLOW_GIT_REV")
    );
    assert!(manifest.contains(&rustflow_line));
    assert!(manifest.contains(&server_line));
    let manifest = manifest
        .replace(
            &rustflow_line,
            &format!(
                "rustflow = {{ path = \"{}\" }}",
                toml_path(&root.join("crates/rustflow"))
            ),
        )
        .replace(
            &server_line,
            &format!(
                "rustflow-server = {{ path = \"{}\" }}",
                toml_path(&root.join("crates/rustflow-server"))
            ),
        );
    fs::write(manifest_path, manifest).unwrap();
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn cargo_target_dir(temp: &TempDir) -> PathBuf {
    temp.path().join("cargo-target")
}

fn generated_http_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to generated server");
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
    stream.write_all(request.as_bytes()).expect("write request");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).expect("read response");
    String::from_utf8(bytes).expect("response is UTF-8")
}

fn generated_response_status(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .expect("HTTP status")
}

fn generated_response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("HTTP response body")
}

fn generated_response_cookie(response: &str, name: &str) -> String {
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

struct ChildGuard(Child);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn creates_a_project_and_generates_all_five_file_types_without_overwrite() {
    let temp = TempDir::new("all-generators");
    let project = create_project(temp.path(), "sample-app");

    for path in [
        "Cargo.toml",
        "README.md",
        ".gitignore",
        ".env.example",
        "src/main.rs",
        "src/bin/migrate.rs",
        "src/components/mod.rs",
        "src/pages/home.rs",
        "src/controllers/home_controller.rs",
        "src/models/mod.rs",
        "migrations/.gitkeep",
    ] {
        assert!(project.join(path).is_file(), "missing {path}");
    }

    let env_example = fs::read_to_string(project.join(".env.example")).unwrap();
    assert!(env_example.contains("RUSTFLOW_ENV=development"));
    assert!(env_example.contains("RUSTFLOW_HOST=127.0.0.1"));
    assert!(env_example.contains("RUSTFLOW_PORT=3000"));
    assert!(env_example.contains("RUSTFLOW_DATABASE_URL=sqlite://rustflow.db?mode=rwc"));
    assert!(env_example.contains("RUSTFLOW_DATABASE_MAX_CONNECTIONS=10"));
    assert!(env_example.contains("sqlite:, postgres://, postgresql://, or mysql://"));
    assert!(env_example.contains("SQL placeholders and migrations are engine-specific"));
    assert!(env_example.contains("MongoConfig/MongoDatabase"));
    assert!(env_example.contains("mongodb:// URI"));
    for forbidden in [
        "PASSWORD=",
        "TOKEN=",
        "SECRET_KEY=",
        "user:password",
        "root:root",
    ] {
        assert!(
            !env_example.contains(forbidden),
            "unsafe value in .env.example: {forbidden}"
        );
    }
    let generated_readme = fs::read_to_string(project.join("README.md")).unwrap();
    assert!(generated_readme.contains("docs/DATABASE_SUPPORT_MATRIX.md"));
    assert!(generated_readme.contains("no hace portables los placeholders"));
    assert!(generated_readme.contains("MongoDB usa una API documental independiente"));
    let generated_ignore = fs::read_to_string(project.join(".gitignore")).unwrap();
    assert!(generated_ignore.contains(".env.*"));
    assert!(generated_ignore.contains("!.env.example"));
    let generated_main = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(generated_main.contains("AppConfig::load()?"));

    for (kind, name, target, expected) in [
        (
            "component",
            "ProductCard",
            "src/components/product_card.rs",
            "View::element",
        ),
        ("page", "about", "src/pages/about.rs", "pub fn render"),
        (
            "controller",
            "health",
            "src/controllers/health_controller.rs",
            "pub async fn index",
        ),
        ("model", "User", "src/models/user.rs", "pub struct User"),
        (
            "migration",
            "create_users",
            "migrations/0001_create_users.sql",
            "CREATE TABLE IF NOT EXISTS users",
        ),
    ] {
        let output = invoke(&project, &["generate", kind, name]);
        assert_success(&output);
        let contents = fs::read_to_string(project.join(target)).unwrap();
        assert!(
            contents.contains(expected),
            "{target} did not contain {expected}"
        );
    }

    let second_migration = invoke(&project, &["generate", "migration", "add_email"]);
    assert_success(&second_migration);
    assert!(project.join("migrations/0002_add_email.sql").is_file());

    let component_path = project.join("src/components/product_card.rs");
    fs::write(&component_path, "user-owned contents\n").unwrap();
    let collision = invoke(&project, &["generate", "component", "ProductCard"]);
    assert_eq!(collision.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(component_path).unwrap(),
        "user-owned contents\n"
    );
    assert!(String::from_utf8_lossy(&collision.stderr).contains("refusing to overwrite"));
}

#[test]
fn rejects_invalid_names_and_existing_project_destinations_before_changes() {
    let temp = TempDir::new("project-collisions");

    let unknown_template = invoke(
        temp.path(),
        &["new", "unknown-template", "--template", "not-a-template"],
    );
    assert_eq!(unknown_template.status.code(), Some(2));
    assert!(!temp.path().join("unknown-template").exists());
    let missing_template = invoke(temp.path(), &["new", "missing-template", "--template"]);
    assert_eq!(missing_template.status.code(), Some(2));
    assert!(!temp.path().join("missing-template").exists());

    for invalid in ["../escape", "9invalid", "bad/name", "bad name"] {
        let output = invoke(temp.path(), &["new", invalid]);
        assert_eq!(output.status.code(), Some(2), "{invalid}");
    }
    assert!(!temp.path().join("escape").exists());

    fs::write(temp.path().join("taken"), "existing file").unwrap();
    let collision = invoke(temp.path(), &["new", "taken"]);
    assert_eq!(collision.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(temp.path().join("taken")).unwrap(),
        "existing file"
    );

    fs::create_dir(temp.path().join("taken-dir")).unwrap();
    fs::write(temp.path().join("taken-dir/user.txt"), "keep").unwrap();
    let collision = invoke(temp.path(), &["new", "taken-dir"]);
    assert_eq!(collision.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(temp.path().join("taken-dir/user.txt")).unwrap(),
        "keep"
    );

    let invalid_parent = temp.path().join("does-not-exist");
    let output = cli()
        .current_dir(temp.path())
        .args(["new", "fresh", "--path"])
        .arg(invalid_parent)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!temp.path().join("fresh").exists());

    assert!(
        fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".rustflow-new-"))
    );
}

#[test]
fn generator_templates_and_module_indexes_are_deterministic() {
    let first = TempDir::new("determinism-a");
    let second = TempDir::new("determinism-b");
    let project_a = create_project(first.path(), "determinism-a");
    let project_b = create_project(second.path(), "determinism-b");

    for kind_name in [
        ("component", "ProductCard"),
        ("page", "about"),
        ("controller", "health"),
        ("model", "User"),
        ("migration", "create_users"),
    ] {
        assert_success(&invoke(&project_a, &["generate", kind_name.0, kind_name.1]));
        assert_success(&invoke(&project_b, &["generate", kind_name.0, kind_name.1]));
    }

    for path in [
        "src/components/product_card.rs",
        "src/pages/about.rs",
        "src/controllers/health_controller.rs",
        "src/models/user.rs",
        "migrations/0001_create_users.sql",
    ] {
        assert_eq!(
            fs::read(project_a.join(path)).unwrap(),
            fs::read(project_b.join(path)).unwrap(),
            "template {path} must not depend on project path or time"
        );
    }
    assert_eq!(
        fs::read(project_a.join("src/components/mod.rs")).unwrap(),
        fs::read(project_b.join("src/components/mod.rs")).unwrap()
    );
}

#[test]
fn build_compiles_a_generated_project_and_its_server_can_serve_http() {
    let temp = TempDir::new("clean-build");
    let project = create_project(temp.path(), "cli-fixture-app");
    for (kind, name) in [
        ("component", "ProductCard"),
        ("page", "about"),
        ("controller", "health"),
        ("model", "User"),
        ("migration", "create_users"),
    ] {
        assert_success(&invoke(&project, &["generate", kind, name]));
    }
    let manifest_path = project.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        format!(
            "{manifest}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nhttp = \"1\"\n"
        ),
    )
    .unwrap();
    fs::write(project.join("src/main.rs"), GENERATED_CRUD_MAIN).unwrap();
    fs::write(
        project.join("migrations/0001_create_users.sql"),
        "CREATE TABLE notes (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL UNIQUE);\n",
    )
    .unwrap();

    localize_framework_dependencies(&project);
    let target_dir = cargo_target_dir(&temp);

    let output = cli()
        .current_dir(&project)
        .args(["build", "--offline"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run rustflow build");
    assert_success(&output);

    let binary = target_dir.join("debug").join("cli-fixture-app");
    assert!(
        binary.is_file(),
        "expected generated executable at {}",
        binary.display()
    );
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(&binary)
        .current_dir(&project)
        .env("RUSTFLOW_ENV", "test")
        .env("RUSTFLOW_HOST", "127.0.0.1")
        .env("RUSTFLOW_PORT", port.to_string())
        .env("RUSTFLOW_DATABASE_URL", "sqlite://rustflow.db?mode=rwc")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start generated server");
    let mut child = ChildGuard::new(child);

    let mut response = None;
    for _ in 0..200 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            response = Some(String::from_utf8_lossy(&bytes).into_owned());
            break;
        }
        if let Ok(Some(_)) = child.0.try_wait() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let response = response.expect("generated server became reachable");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("Hola, RustFlow"), "{response}");

    let health = generated_http_request(port, "GET", "/health/database", &[], "");
    assert_eq!(generated_response_status(&health), 200, "{health}");
    assert_eq!(generated_response_body(&health), "ok");

    let anonymous = generated_http_request(port, "GET", "/notes", &[], "");
    assert_eq!(generated_response_status(&anonymous), 401, "{anonymous}");

    let csrf = generated_http_request(port, "GET", "/csrf", &[], "");
    assert_eq!(generated_response_status(&csrf), 200, "{csrf}");
    let csrf_token = generated_response_body(&csrf).to_owned();
    let csrf_cookie = generated_response_cookie(&csrf, "rustflow_csrf");

    let login_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", csrf_cookie.as_str()),
        ("x-csrf-token", csrf_token.as_str()),
    ];
    let wrong_login = generated_http_request(
        port,
        "POST",
        "/login",
        &login_headers,
        "username=ada&password=incorrect",
    );
    assert_eq!(
        generated_response_status(&wrong_login),
        401,
        "{wrong_login}"
    );

    let login = generated_http_request(
        port,
        "POST",
        "/login",
        &login_headers,
        "username=ada&password=correct%20horse%20battery%20staple",
    );
    assert_eq!(generated_response_status(&login), 200, "{login}");
    let session_cookie = generated_response_cookie(&login, "rustflow_session");
    let cookies = format!("{csrf_cookie}; {session_cookie}");
    let secured_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", cookies.as_str()),
        ("x-csrf-token", csrf_token.as_str()),
    ];

    let invalid =
        generated_http_request(port, "POST", "/notes", &secured_headers, "title=%20%20%20");
    assert_eq!(generated_response_status(&invalid), 422, "{invalid}");

    let created = generated_http_request(
        port,
        "POST",
        "/notes",
        &secured_headers,
        "title=Ledger%20%3Cscript%3E",
    );
    assert_eq!(generated_response_status(&created), 303, "{created}");

    let notes = generated_http_request(port, "GET", "/notes", &[("cookie", cookies.as_str())], "");
    assert_eq!(generated_response_status(&notes), 200, "{notes}");
    assert!(
        generated_response_body(&notes).contains("Ledger &lt;script&gt;"),
        "{notes}"
    );
    assert!(
        !generated_response_body(&notes).contains("<script>"),
        "{notes}"
    );
    assert_eq!(
        generated_response_body(&notes).matches("<li>").count(),
        1,
        "only a successfully validated note should be persisted: {notes}"
    );
}

fn exercise_generated_crud_backend(binary: &Path, project: &Path, database_url: &str, label: &str) {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(binary)
        .current_dir(project)
        .env("RUSTFLOW_ENV", "test")
        .env("RUSTFLOW_HOST", "127.0.0.1")
        .env("RUSTFLOW_PORT", port.to_string())
        .env("RUSTFLOW_DATABASE_URL", database_url)
        .env("RUSTFLOW_DEMO_USER", "admin")
        .env("RUSTFLOW_DEMO_PASSWORD", "generated-crud-test-password")
        .env(
            "RUSTFLOW_CSRF_KEY",
            "test-only-generated-crud-csrf-key-minimum-32-bytes",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("start generated CRUD on {label}: {error}"));
    let mut child = ChildGuard::new(child);

    let mut ready = false;
    for _ in 0..240 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        if let Ok(Some(status)) = child.0.try_wait() {
            panic!("generated CRUD exited on {label} before listening: {status}");
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(ready, "generated CRUD did not start on {label}");

    let anonymous = generated_http_request(port, "GET", "/products", &[], "");
    assert_eq!(
        generated_response_status(&anonymous),
        401,
        "{label}: {anonymous}"
    );
    let login_page = generated_http_request(port, "GET", "/login", &[], "");
    assert_eq!(
        generated_response_status(&login_page),
        200,
        "{label}: {login_page}"
    );
    let csrf_cookie = generated_response_cookie(&login_page, "rustflow_csrf");
    let csrf_token = generated_response_body(&login_page)
        .split_once("name=\"_csrf\" value=\"")
        .and_then(|(_, rest)| rest.split('"').next())
        .expect("login page emits CSRF token")
        .to_owned();
    let login_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", csrf_cookie.as_str()),
    ];
    let login_body = form_body(&[
        ("_csrf", &csrf_token),
        ("username", "admin"),
        ("password", "generated-crud-test-password"),
    ]);
    let login = generated_http_request(port, "POST", "/login", &login_headers, &login_body);
    assert_eq!(generated_response_status(&login), 303, "{label}: {login}");
    let session_cookie = generated_response_cookie(&login, "rustflow_session");
    let cookies = format!("{csrf_cookie}; {session_cookie}");
    let secured_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", cookies.as_str()),
    ];

    let invalid_category_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "Invalid category probe"),
        ("sku", &format!("invalid-category-{label}")),
        ("description", ""),
        ("price_cents", "10"),
        ("category_id", "9999"),
    ]);
    let invalid_category = generated_http_request(
        port,
        "POST",
        "/products",
        &secured_headers,
        &invalid_category_body,
    );
    assert_eq!(
        generated_response_status(&invalid_category),
        409,
        "{label}: foreign-key constraint should map to conflict: {invalid_category}"
    );

    let invalid_filter = generated_http_request(
        port,
        "GET",
        "/products?category_id=-1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&invalid_filter),
        422,
        "{label}: invalid category filter should fail validation: {invalid_filter}"
    );
    let invalid_page = generated_http_request(
        port,
        "GET",
        "/products?page=0",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&invalid_page),
        422,
        "{label}: page zero should fail validation: {invalid_page}"
    );

    let sku = format!("cross-engine-{label}");
    let name = format!("Cross engine {label}");
    let create_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", &name),
        ("sku", &sku),
        ("description", "Generated application smoke test"),
        ("price_cents", "2100"),
        ("category_id", "2"),
    ]);
    let create = generated_http_request(port, "POST", "/products", &secured_headers, &create_body);
    assert_eq!(generated_response_status(&create), 303, "{label}: {create}");

    let list = generated_http_request(
        port,
        "GET",
        "/products",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&list), 200, "{label}: {list}");
    let html = generated_response_body(&list);
    let position = html
        .find(&name)
        .unwrap_or_else(|| panic!("created product missing from {label} list: {html}"));
    let link_start = html[..position]
        .rfind("href=\"/products/")
        .unwrap_or_else(|| panic!("product detail link missing on {label}: {html}"));
    let id_start = link_start + "href=\"/products/".len();
    let id = html[id_start..]
        .split('"')
        .next()
        .expect("product identifier in link")
        .to_owned();
    let detail = generated_http_request(
        port,
        "GET",
        &format!("/products/{id}"),
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&detail), 200, "{label}: {detail}");
    assert!(
        generated_response_body(&detail).contains("Category: Office"),
        "{label}: relation was not rendered by the product detail route: {detail}"
    );

    let filtered = generated_http_request(
        port,
        "GET",
        "/products?q=Cross&category_id=2&page=1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&filtered), 200, "{label}: {filtered}");
    assert!(generated_response_body(&filtered).contains(&name), "{label}: {filtered}");

    for index in 1..=6 {
        let batch_name = format!("Batch item {index:02}");
        let batch_sku = format!("batch-{label}-{index:02}");
        let batch_body = form_body(&[
            ("_csrf", &csrf_token),
            ("name", &batch_name),
            ("sku", &batch_sku),
            ("description", ""),
            ("price_cents", "50"),
            ("category_id", "1"),
        ]);
        let response = generated_http_request(
            port,
            "POST",
            "/products",
            &secured_headers,
            &batch_body,
        );
        assert_eq!(
            generated_response_status(&response),
            303,
            "{label}: batch item creation failed: {response}"
        );
    }

    let first_page = generated_http_request(
        port,
        "GET",
        "/products?q=Batch%20item&category_id=1&page=1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&first_page), 200, "{label}: {first_page}");
    let first_page_body = generated_response_body(&first_page);
    assert!(first_page_body.contains("Batch item 01"), "{label}: {first_page}");
    assert!(first_page_body.contains("Batch item 05"), "{label}: {first_page}");
    assert!(!first_page_body.contains("Batch item 06"), "{label}: {first_page}");

    let second_page = generated_http_request(
        port,
        "GET",
        "/products?q=Batch%20item&category_id=1&page=2",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&second_page), 200, "{label}: {second_page}");
    let second_page_body = generated_response_body(&second_page);
    assert!(second_page_body.contains("Batch item 06"), "{label}: {second_page}");
    assert!(!second_page_body.contains("Batch item 01"), "{label}: {second_page}");

    for index in 1..=6 {
        let batch_name = format!("Batch item {index:02}");
        let source = if index <= 5 { first_page_body } else { second_page_body };
        let batch_id = generated_product_id_from_list(source, &batch_name);
        let delete_body = form_body(&[("_csrf", &csrf_token)]);
        let response = generated_http_request(
            port,
            "POST",
            &format!("/products/{batch_id}/delete"),
            &secured_headers,
            &delete_body,
        );
        assert_eq!(
            generated_response_status(&response),
            303,
            "{label}: batch item deletion failed: {response}"
        );
    }

    let updated_name = format!("Updated {name}");
    let update_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", &updated_name),
        ("sku", &sku),
        ("description", "Updated on SQL engine"),
        ("price_cents", "2500"),
        ("category_id", "2"),
    ]);
    let update = generated_http_request(
        port,
        "POST",
        &format!("/products/{id}"),
        &secured_headers,
        &update_body,
    );
    assert_eq!(generated_response_status(&update), 303, "{label}: {update}");
    let updated = generated_http_request(
        port,
        "GET",
        &format!("/products/{id}"),
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&updated),
        200,
        "{label}: {updated}"
    );
    assert!(
        generated_response_body(&updated).contains(&updated_name),
        "{label}: {updated}"
    );

    let probe_body = form_body(&[("_csrf", &csrf_token)]);
    let rollback = generated_http_request(
        port,
        "POST",
        "/admin/transaction-rollback-probe",
        &secured_headers,
        &probe_body,
    );
    assert_eq!(
        generated_response_status(&rollback),
        409,
        "{label}: {rollback}"
    );
    let after_rollback = generated_http_request(
        port,
        "GET",
        "/products",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&after_rollback),
        200,
        "{label}: {after_rollback}"
    );
    assert!(!generated_response_body(&after_rollback).contains("Rollback probe"));

    let delete = generated_http_request(
        port,
        "POST",
        &format!("/products/{id}/delete"),
        &secured_headers,
        &probe_body,
    );
    assert_eq!(generated_response_status(&delete), 303, "{label}: {delete}");
    let deleted = generated_http_request(
        port,
        "GET",
        &format!("/products/{id}"),
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&deleted),
        404,
        "{label}: {deleted}"
    );
}

fn form_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn form_body(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", form_component(name), form_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn generated_product_id_from_list(html: &str, name: &str) -> String {
    let position = html
        .find(name)
        .unwrap_or_else(|| panic!("product {name} missing from generated listing: {html}"));
    let link_start = html[..position]
        .rfind("href=\"/products/")
        .unwrap_or_else(|| panic!("product detail link missing for {name}: {html}"));
    let id_start = link_start + "href=\"/products/".len();
    html[id_start..]
        .split('"')
        .next()
        .expect("product identifier in generated listing")
        .to_owned()
}

#[test]
fn generated_crud_template_builds_and_runs_the_real_orm_application() {
    let temp = TempDir::new("generated-crud");
    let creation = invoke(
        temp.path(),
        &["new", "generated-crud-app", "--template", "crud"],
    );
    assert_success(&creation);
    let project = temp.path().join("generated-crud-app");

    let snapshot_temp = TempDir::new("generated-crud-snapshot");
    let snapshot_create = invoke(
        snapshot_temp.path(),
        &["new", "generated-crud-app", "--template", "crud"],
    );
    assert_success(&snapshot_create);
    let snapshot_project = snapshot_temp.path().join("generated-crud-app");
    for path in [
        "Cargo.toml",
        "README.md",
        ".env.example",
        "src/main.rs",
        "src/models/category.rs",
        "src/models/product.rs",
        "migrations/sqlite/20261031010001_create_products.sql",
        "migrations/postgres/20261031010002_create_products.sql",
        "migrations/mysql/20261031010003_create_products.sql",
    ] {
        assert_eq!(
            fs::read(project.join(path)).unwrap(),
            fs::read(snapshot_project.join(path)).unwrap(),
            "CRUD scaffold snapshot drift for {path}"
        );
    }

    for path in [
        "src/main.rs",
        "src/models/category.rs",
        "src/models/product.rs",
        "src/models/mod.rs",
        "src/bin/migrate.rs",
        "migrations/sqlite/20261031010001_create_products.sql",
        "migrations/postgres/20261031010002_create_products.sql",
        "migrations/mysql/20261031010003_create_products.sql",
        "migrations/mysql/.rustflow-dialect",
    ] {
        assert!(
            project.join(path).is_file(),
            "missing generated CRUD file {path}"
        );
    }
    let generated_main = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(
        generated_main.contains("DatabasePool::entity")
            || generated_main.contains("entity::<ProductEntity>")
    );
    assert!(generated_main.contains("validated_form::<ProductForm>"));
    assert!(generated_main.contains("begin_transaction"));
    assert!(generated_main.contains("route_authorized"));
    assert!(generated_main.contains("CsrfProtection"));
    assert!(generated_main.contains("html_document"));
    assert!(generated_main.contains("validated_query::<ProductListQuery>"));
    assert!(generated_main.contains("load_one"));
    assert!(!generated_main.contains("sqlx::query("));

    let generated_entity = fs::read_to_string(project.join("src/models/product.rs")).unwrap();
    assert!(generated_entity.contains("ModelDiagnostics"));
    assert!(generated_entity.contains("DeriveEntityModel"));
    assert!(generated_entity.contains("CategoryId"));
    assert!(generated_entity.contains("belongs_to"));
    let generated_category = fs::read_to_string(project.join("src/models/category.rs")).unwrap();
    assert!(generated_category.contains("has_many"));
    let generated_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(generated_manifest.contains("serde = { version = \"1\", features = [\"derive\"] }"));

    localize_framework_dependencies(&project);
    let target_dir = cargo_target_dir(&temp);
    let build = cli()
        .current_dir(&project)
        .args(["build", "--offline"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("build generated CRUD application");
    assert_success(&build);

    let binary = target_dir.join("debug").join("generated-crud-app");
    assert!(binary.is_file(), "generated CRUD binary was not built");
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(&binary)
        .current_dir(&project)
        .env("RUSTFLOW_ENV", "test")
        .env("RUSTFLOW_HOST", "127.0.0.1")
        .env("RUSTFLOW_PORT", port.to_string())
        .env("RUSTFLOW_DATABASE_URL", "sqlite://rustflow.db?mode=rwc")
        .env("RUSTFLOW_DEMO_USER", "admin")
        .env("RUSTFLOW_DEMO_PASSWORD", "generated-crud-test-password")
        .env(
            "RUSTFLOW_CSRF_KEY",
            "test-only-generated-crud-csrf-key-minimum-32-bytes",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start generated CRUD server");
    let mut child = ChildGuard::new(child);

    let mut ready = false;
    for _ in 0..200 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        if let Ok(Some(status)) = child.0.try_wait() {
            panic!("generated CRUD server exited before listening: {status}");
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(ready, "generated CRUD server did not become reachable");

    let anonymous = generated_http_request(port, "GET", "/products", &[], "");
    assert_eq!(generated_response_status(&anonymous), 401, "{anonymous}");

    let login_page = generated_http_request(port, "GET", "/login", &[], "");
    assert_eq!(generated_response_status(&login_page), 200, "{login_page}");
    let csrf_cookie = generated_response_cookie(&login_page, "rustflow_csrf");
    let marker = "name=\"_csrf\" value=\"";
    let csrf_token = generated_response_body(&login_page)
        .split_once(marker)
        .and_then(|(_, rest)| rest.split('"').next())
        .expect("login page renders a hidden CSRF token")
        .to_owned();

    let login_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", csrf_cookie.as_str()),
    ];
    let wrong_login_body = form_body(&[
        ("_csrf", &csrf_token),
        ("username", "admin"),
        ("password", "wrong-password"),
    ]);
    let wrong_login =
        generated_http_request(port, "POST", "/login", &login_headers, &wrong_login_body);
    assert_eq!(
        generated_response_status(&wrong_login),
        401,
        "{wrong_login}"
    );

    let login_body = form_body(&[
        ("_csrf", &csrf_token),
        ("username", "admin"),
        ("password", "generated-crud-test-password"),
    ]);
    let login = generated_http_request(port, "POST", "/login", &login_headers, &login_body);
    assert_eq!(generated_response_status(&login), 303, "{login}");
    let session_cookie = generated_response_cookie(&login, "rustflow_session");
    let cookies = format!("{csrf_cookie}; {session_cookie}");
    let authenticated_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", cookies.as_str()),
    ];

    let products = generated_http_request(
        port,
        "GET",
        "/products",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&products), 200, "{products}");
    assert!(generated_response_body(&products).contains("No products have been created yet."));

    let invalid_filter = generated_http_request(
        port,
        "GET",
        "/products?category_id=-1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&invalid_filter),
        422,
        "{invalid_filter}"
    );
    let invalid_page = generated_http_request(
        port,
        "GET",
        "/products?page=0",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&invalid_page),
        422,
        "{invalid_page}"
    );

    let invalid_category_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "Invalid category probe"),
        ("sku", "invalid-category-sqlite"),
        ("description", ""),
        ("price_cents", "10"),
        ("category_id", "9999"),
    ]);
    let invalid_category = generated_http_request(
        port,
        "POST",
        "/products",
        &authenticated_headers,
        &invalid_category_body,
    );
    assert_eq!(
        generated_response_status(&invalid_category),
        409,
        "{invalid_category}"
    );

    let denied = generated_http_request(
        port,
        "GET",
        "/admin/denied-demo",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&denied), 403, "{denied}");

    let missing_csrf = generated_http_request(
        port,
        "POST",
        "/products",
        &authenticated_headers,
        "name=No+token",
    );
    assert_eq!(
        generated_response_status(&missing_csrf),
        403,
        "{missing_csrf}"
    );

    let invalid_csrf_headers = [
        ("content-type", "application/x-www-form-urlencoded"),
        ("origin", "http://localhost"),
        ("cookie", cookies.as_str()),
        ("x-csrf-token", "invalid-token"),
    ];
    let invalid_csrf = generated_http_request(port, "POST", "/products", &invalid_csrf_headers, "");
    assert_eq!(
        generated_response_status(&invalid_csrf),
        403,
        "{invalid_csrf}"
    );

    let invalid_form_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "   "),
        ("sku", ""),
        ("description", ""),
        ("price_cents", "-1"),
        ("category_id", "1"),
    ]);
    let invalid_form = generated_http_request(
        port,
        "POST",
        "/products",
        &authenticated_headers,
        &invalid_form_body,
    );
    assert_eq!(
        generated_response_status(&invalid_form),
        422,
        "{invalid_form}"
    );
    assert!(generated_response_body(&invalid_form).contains("validation_failed"));

    let malformed_form_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "Malformed"),
        ("sku", "bad-price"),
        ("description", ""),
        ("price_cents", "not-an-integer"),
        ("category_id", "1"),
    ]);
    let malformed_form = generated_http_request(
        port,
        "POST",
        "/products",
        &authenticated_headers,
        &malformed_form_body,
    );
    assert_eq!(
        generated_response_status(&malformed_form),
        400,
        "{malformed_form}"
    );

    let create_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "Desk <script>alert(1)</script>"),
        ("sku", "desk-001"),
        ("description", "User supplied <script> text"),
        ("price_cents", "1250"),
        ("category_id", "2"),
    ]);
    let created = generated_http_request(
        port,
        "POST",
        "/products",
        &authenticated_headers,
        &create_body,
    );
    assert_eq!(generated_response_status(&created), 303, "{created}");

    let list_after_create = generated_http_request(
        port,
        "GET",
        "/products",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&list_after_create),
        200,
        "{list_after_create}"
    );
    assert!(
        generated_response_body(&list_after_create)
            .contains("Desk &lt;script&gt;alert(1)&lt;/script&gt;")
    );
    assert!(!generated_response_body(&list_after_create).contains("<script>"));

    let detail = generated_http_request(
        port,
        "GET",
        "/products/1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&detail), 200, "{detail}");
    assert!(generated_response_body(&detail).contains("User supplied &lt;script&gt; text"));
    assert!(generated_response_body(&detail).contains("Category: Office"));

    let filtered_products = generated_http_request(
        port,
        "GET",
        "/products?q=Desk&category_id=2&page=1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&filtered_products),
        200,
        "{filtered_products}"
    );
    assert!(generated_response_body(&filtered_products).contains("Desk"));
    assert!(!generated_response_body(&filtered_products).contains("Batch item"));

    for index in 1..=6 {
        let name = format!("Batch item {index:02}");
        let sku = format!("batch-{index:02}");
        let body = form_body(&[
            ("_csrf", &csrf_token),
            ("name", &name),
            ("sku", &sku),
            ("description", ""),
            ("price_cents", "50"),
            ("category_id", "1"),
        ]);
        let response = generated_http_request(
            port,
            "POST",
            "/products",
            &authenticated_headers,
            &body,
        );
        assert_eq!(
            generated_response_status(&response),
            303,
            "batch product creation failed: {response}"
        );
    }

    let first_page = generated_http_request(
        port,
        "GET",
        "/products?q=Batch%20item&category_id=1&page=1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&first_page), 200, "{first_page}");
    let first_page_body = generated_response_body(&first_page);
    assert!(first_page_body.contains("Batch item 01"));
    assert!(first_page_body.contains("Batch item 05"));
    assert!(!first_page_body.contains("Batch item 06"));

    let second_page = generated_http_request(
        port,
        "GET",
        "/products?q=Batch%20item&category_id=1&page=2",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&second_page), 200, "{second_page}");
    let second_page_body = generated_response_body(&second_page);
    assert!(second_page_body.contains("Batch item 06"));
    assert!(!second_page_body.contains("Batch item 01"));

    let invalid_id = generated_http_request(
        port,
        "GET",
        "/products/not-an-id",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(generated_response_status(&invalid_id), 400, "{invalid_id}");
    let missing_product = generated_http_request(
        port,
        "GET",
        "/products/9999",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&missing_product),
        404,
        "{missing_product}"
    );

    let update_body = form_body(&[
        ("_csrf", &csrf_token),
        ("name", "Desk Pro"),
        ("sku", "desk-001"),
        ("description", "Updated product"),
        ("price_cents", "1499"),
        ("category_id", "2"),
    ]);
    let updated = generated_http_request(
        port,
        "POST",
        "/products/1",
        &authenticated_headers,
        &update_body,
    );
    assert_eq!(generated_response_status(&updated), 303, "{updated}");
    let updated_detail = generated_http_request(
        port,
        "GET",
        "/products/1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&updated_detail),
        200,
        "{updated_detail}"
    );
    assert!(generated_response_body(&updated_detail).contains("Desk Pro"));
    assert!(generated_response_body(&updated_detail).contains("1499 cents"));

    let rollback_body = form_body(&[("_csrf", &csrf_token)]);
    let rollback = generated_http_request(
        port,
        "POST",
        "/admin/transaction-rollback-probe",
        &authenticated_headers,
        &rollback_body,
    );
    assert_eq!(generated_response_status(&rollback), 409, "{rollback}");
    let after_rollback = generated_http_request(
        port,
        "GET",
        "/products",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&after_rollback),
        200,
        "{after_rollback}"
    );
    assert!(!generated_response_body(&after_rollback).contains("Rollback probe"));

    let deleted = generated_http_request(
        port,
        "POST",
        "/products/1/delete",
        &authenticated_headers,
        &rollback_body,
    );
    assert_eq!(generated_response_status(&deleted), 303, "{deleted}");
    let after_delete = generated_http_request(
        port,
        "GET",
        "/products/1",
        &[("cookie", cookies.as_str())],
        "",
    );
    assert_eq!(
        generated_response_status(&after_delete),
        404,
        "{after_delete}"
    );
    drop(child);

    for (variable, label) in [
        ("RUSTFLOW_TEST_DATABASE_URL", "postgres"),
        ("RUSTFLOW_TEST_MYSQL_URL", "mysql"),
        ("RUSTFLOW_TEST_MARIADB_URL", "mariadb"),
    ] {
        match env::var(variable) {
            Ok(url) if !url.trim().is_empty() => {
                exercise_generated_crud_backend(&binary, &project, &url, label);
            }
            _ if env::var("CI").is_ok_and(|value| value == "true") => {
                panic!("{variable} must be configured in CI for generated CRUD engine coverage");
            }
            _ => eprintln!(
                "Skipping generated CRUD {label} contract outside CI: {variable} is unset"
            ),
        }
    }
}

#[test]
fn dev_runs_cargo_and_invalid_projects_return_a_clear_error() {
    let temp = TempDir::new("dev-contract");
    let outside = invoke(temp.path(), &["dev"]);
    assert_eq!(outside.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&outside.stderr).contains("no RustFlow application"));
    let outside_build = invoke(temp.path(), &["build"]);
    assert_eq!(outside_build.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&outside_build.stderr).contains("no RustFlow application"));

    let project = create_project(temp.path(), "dev-fixture-app");
    let cargo_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_manifest.contains("default-run = \"dev-fixture-app\""));
    localize_framework_dependencies(&project);
    let target_dir = cargo_target_dir(&temp);
    fs::write(
        project.join("src/main.rs"),
        "fn main() { println!(\"dev executed\"); }\n",
    )
    .unwrap();

    let output = cli()
        .current_dir(&project)
        .args(["dev", "--offline"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run rustflow dev");
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("dev executed"));

    fs::write(project.join("src/main.rs"), "fn main( { }\n").unwrap();
    let build_failure = cli()
        .current_dir(&project)
        .args(["build", "--offline"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run failing rustflow build");
    assert_eq!(
        build_failure.status.code(),
        Some(101),
        "expected Cargo's compilation failure status, got stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_failure.stdout),
        String::from_utf8_lossy(&build_failure.stderr)
    );
    assert!(String::from_utf8_lossy(&build_failure.stderr).contains("error"));
}

#[test]
fn rejects_invalid_generator_names_and_preserves_unmanaged_module_indexes() {
    let temp = TempDir::new("module-protection");
    let project = create_project(temp.path(), "module-protection-app");
    let index = project.join("src/components/mod.rs");
    fs::write(&index, "pub mod user_owned;\n").unwrap();

    let output = invoke(&project, &["generate", "component", "NewCard"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!project.join("src/components/new_card.rs").exists());
    assert_eq!(fs::read_to_string(index).unwrap(), "pub mod user_owned;\n");

    let invalid = invoke(&project, &["generate", "model", "type"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(!project.join("src/models/type.rs").exists());
}
