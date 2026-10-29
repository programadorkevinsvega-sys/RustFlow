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
                    .execute(database.pool())
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
            .fetch_all(database.pool())
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
                .execute(database.pool())
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
    let child = Command::new(binary)
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
