//! Command-line tooling for creating and working with RustFlow applications.
//!
//! The CLI deliberately delegates compilation and execution to Cargo. It does
//! not implement file watching, hot reload, or a browser/WASM bundler.

use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const REPOSITORY_URL: &str = "https://github.com/programadorkevinsvega-sys/RustFlow.git";
const MODULE_START: &str = "// rustflow:managed-modules:start";
const MODULE_END: &str = "// rustflow:managed-modules:end";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A user-facing CLI error. Errors are printed by the binary with exit status 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for CliError {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Run the CLI arguments excluding the executable name.
///
/// Returns the process exit code. Cargo exit codes are preserved instead of
/// being converted to success or hidden behind a generic CLI status.
pub fn run<I, S>(args: I) -> Result<i32, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    if args.is_empty() || is_help(&args[0]) {
        print_help();
        return Ok(0);
    }

    match argument_text(&args[0], "command")? {
        "new" => new_command(&args[1..]),
        "dev" => dev_command(&args[1..]),
        "build" => build_command(&args[1..]),
        "generate" | "g" => generate_command(&args[1..]),
        "help" => {
            print_help();
            Ok(0)
        }
        "version" | "--version" | "-V" => {
            println!("rustflow {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        command => {
            Err(format!("unknown command `{command}`\n\nRun `rustflow --help` for usage.").into())
        }
    }
}

fn print_help() {
    println!(
        "RustFlow CLI {}\n\nUSAGE:\n  rustflow <COMMAND> [OPTIONS]\n\nCOMMANDS:\n  new <name> [--path <parent>] [--template server|crud]  Create a RustFlow application\n  dev [--release] [--offline]        Run the application through Cargo\n  build [OPTIONS]                    Build the application through Cargo\n  generate <TYPE> <name>              Generate a component, page, controller, model, or migration\n  help                                Show this help\n\nGENERATOR TYPES:\n  component | page | controller | model | migration\n\nNOTES:\n  Generators refuse file collisions. dev does not watch files or provide hot reload.\n  Model and migration output is structural; no ORM or migration runner is included.\n  RustFlow is pre-1.0. Generated projects pin the source revision used by this CLI.",
        env!("CARGO_PKG_VERSION")
    );
}

fn new_command(args: &[OsString]) -> Result<i32, CliError> {
    if args.is_empty() || is_help(&args[0]) {
        println!("Usage: rustflow new <name> [--path <existing-parent-directory>] [--template server|crud]");
        return if args.is_empty() {
            Err("missing project name".into())
        } else {
            Ok(0)
        };
    }

    let name = argument_text(&args[0], "project name")?;
    validate_project_name(name)?;
    let mut parent = env::current_dir()
        .map_err(|error| format!("could not determine the current directory: {error}"))?;

    let mut template = "server";
    let mut index = 1;
    while index < args.len() {
        match argument_text(&args[index], "option")? {
            "--path" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("option --path requires an existing parent directory".into());
                };
                parent = PathBuf::from(value);
                index += 2;
            }
            "--template" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("option --template requires server or crud".into());
                };
                template = argument_text(value, "template name")?;
                if !matches!(template, "server" | "crud") {
                    return Err(format!("unknown application template `{template}`; choose server or crud").into());
                }
                index += 2;
            }
            option => return Err(format!("unknown option for rustflow new: {option}").into()),
        }
    }

    let revision = env!("RUSTFLOW_GIT_REV");
    if !is_revision(revision) {
        return Err(
            "cannot determine the RustFlow source revision for this CLI build; install it from a Git checkout or set RUSTFLOW_GIT_REV to a full 40-character commit SHA"
                .into(),
        );
    }

    let destination = create_project(name, &parent, revision, template)?;
    println!(
        "Created RustFlow project `{name}` at {}",
        destination.display()
    );
    println!("Next steps:");
    println!("  cd {}", destination.display());
    println!("  rustflow dev");
    println!("  rustflow build");
    Ok(0)
}

fn dev_command(args: &[OsString]) -> Result<i32, CliError> {
    let mut cargo_args = Vec::new();
    let mut forwarded = Vec::new();
    let mut passthrough = false;

    for arg in args {
        if passthrough {
            forwarded.push(arg.clone());
            continue;
        }
        match argument_text(arg, "dev option")? {
            "--" => {
                passthrough = true;
                continue;
            }
            "--release" | "--offline" | "--locked" => cargo_args.push(arg.clone()),
            "--help" | "-h" => {
                println!(
                    "Usage: rustflow dev [--release] [--offline] [--locked] [-- <application-args>]"
                );
                println!(
                    "Runs `cargo run` in the nearest RustFlow project. No file watcher or hot reload is provided."
                );
                return Ok(0);
            }
            option => return Err(format!("unsupported rustflow dev option: {option}").into()),
        }
    }

    if passthrough {
        cargo_args.push(OsString::from("--"));
        cargo_args.extend(forwarded);
    }

    run_cargo("run", &cargo_args, "dev")
}

fn build_command(args: &[OsString]) -> Result<i32, CliError> {
    let mut cargo_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let option = argument_text(&args[index], "build option")?;
        match option {
            "--release" | "--offline" | "--locked" | "--all-features" | "--no-default-features" => {
                cargo_args.push(args[index].clone());
                index += 1;
            }
            "--target" | "--features" | "-F" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(format!("option {option} requires a value").into());
                };
                cargo_args.push(args[index].clone());
                cargo_args.push(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: rustflow build [--release] [--offline] [--locked] [--target <triple>] [--features <list>] [--all-features]"
                );
                println!(
                    "Delegates to `cargo build`; the project must support the selected target."
                );
                return Ok(0);
            }
            option => return Err(format!("unsupported rustflow build option: {option}").into()),
        }
    }

    let code = run_cargo("build", &cargo_args, "build")?;
    if code == 0 {
        let profile = if cargo_args.iter().any(|arg| arg == "--release") {
            "release"
        } else {
            "debug"
        };
        println!("Cargo build completed successfully (profile: {profile}).");
        println!("Inspect Cargo's output above for the selected target and artifact path.");
    }
    Ok(code)
}

fn generate_command(args: &[OsString]) -> Result<i32, CliError> {
    if args.is_empty() || is_help(&args[0]) {
        println!("Usage: rustflow generate <component|page|controller|model|migration> <name>");
        return if args.is_empty() {
            Err("missing generator type and name".into())
        } else {
            Ok(0)
        };
    }
    if args.len() != 2 {
        return Err(
            "usage: rustflow generate <component|page|controller|model|migration> <name>".into(),
        );
    }

    let kind = argument_text(&args[0], "generator type")?;
    let name = argument_text(&args[1], "generated name")?;
    let root = find_rustflow_project(&env::current_dir().map_err(|error| error.to_string())?)?;
    let path = generate_file(&root, kind, name)?;
    println!(
        "Generated {}",
        path.strip_prefix(&root).unwrap_or(&path).display()
    );
    Ok(0)
}

fn is_help(arg: &OsStr) -> bool {
    matches!(arg.to_str(), Some("--help" | "-h"))
}

fn argument_text<'a>(arg: &'a OsStr, label: &str) -> Result<&'a str, CliError> {
    arg.to_str()
        .ok_or_else(|| format!("{label} must be valid UTF-8").into())
}

fn validate_project_name(name: &str) -> Result<(), CliError> {
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || name.ends_with('-')
        || name.ends_with('_')
    {
        return Err(format!(
            "invalid project name `{name}`; use an ASCII letter first, followed by letters, digits, hyphens, or underscores"
        )
        .into());
    }
    Ok(())
}

fn generated_identifier(name: &str) -> Result<String, CliError> {
    validate_project_name(name)?;
    let chars = name.chars().collect::<Vec<_>>();
    let mut snake = String::new();

    for (index, character) in chars.iter().copied().enumerate() {
        if character == '-' {
            if !snake.ends_with('_') {
                snake.push('_');
            }
            continue;
        }
        if character.is_ascii_uppercase() {
            let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let next = chars.get(index + 1).copied();
            let boundary = previous.is_some_and(|previous| {
                previous.is_ascii_lowercase()
                    || previous.is_ascii_digit()
                    || (previous.is_ascii_uppercase()
                        && next.is_some_and(|character| character.is_ascii_lowercase()))
            });
            if boundary && !snake.ends_with('_') {
                snake.push('_');
            }
            snake.push(character.to_ascii_lowercase());
        } else {
            snake.push(character);
        }
    }

    while snake.ends_with('_') {
        snake.pop();
    }
    if is_rust_keyword(&snake) {
        return Err(format!(
            "`{name}` normalizes to the Rust keyword `{snake}`; choose another name"
        )
        .into());
    }
    Ok(snake)
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "gen"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

fn create_project(name: &str, parent: &Path, revision: &str, template: &str) -> Result<PathBuf, CliError> {
    validate_project_name(name)?;
    if !is_revision(revision) {
        return Err("RustFlow source revision must be a full 40-character commit SHA".into());
    }
    if !matches!(template, "server" | "crud") {
        return Err("application template must be server or crud".into());
    }

    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "project parent {} is not accessible: {error}",
            parent.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!("project parent {} is not a directory", parent.display()).into());
    }

    let destination = parent.join(name);
    if path_exists(&destination)? {
        return Err(format!(
            "destination {} already exists; choose another name or move the existing file/directory yourself",
            destination.display()
        )
        .into());
    }

    let staging = create_staging_dir(&parent, name)?;
    populate_staging_dir(&staging, &scaffold_files(name, revision, template))?;

    match path_exists(&destination) {
        Ok(false) => {}
        Ok(true) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!(
                "destination {} appeared while the project was being created; no existing files were changed",
                destination.display()
            )
            .into());
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }

    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!(
            "could not publish the generated project at {}: {error}",
            destination.display()
        )
        .into());
    }
    Ok(destination)
}

fn path_exists(path: &Path) -> Result<bool, CliError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("could not inspect {}: {error}", path.display()).into()),
    }
}

fn create_staging_dir(parent: &Path, name: &str) -> Result<PathBuf, CliError> {
    for _ in 0..100 {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".rustflow-new-{name}-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "could not create a staging directory in {}: {error}",
                    parent.display()
                )
                .into());
            }
        }
    }
    Err("could not allocate a unique staging directory after 100 attempts".into())
}

fn populate_staging_dir(staging: &Path, files: &[(PathBuf, String)]) -> Result<(), CliError> {
    if let Err(error) = write_project_files(staging, files) {
        if let Err(cleanup_error) = fs::remove_dir_all(staging) {
            return Err(format!(
                "{error}; could not clean partial project directory {}: {cleanup_error}",
                staging.display()
            )
            .into());
        }
        return Err(error);
    }
    Ok(())
}

fn write_project_files(root: &Path, files: &[(PathBuf, String)]) -> Result<(), CliError> {
    for (relative, contents) in files {
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!("unsafe generated path: {}", relative.display()).into());
        }
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("could not create directory {}: {error}", parent.display())
            })?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| format!("could not create {}: {error}", destination.display()))?;
        if let Err(error) = file.write_all(contents.as_bytes()) {
            let _ = fs::remove_file(&destination);
            return Err(format!("could not write {}: {error}", destination.display()).into());
        }
        if let Err(error) = file.flush() {
            let _ = fs::remove_file(&destination);
            return Err(format!("could not flush {}: {error}", destination.display()).into());
        }
    }
    Ok(())
}

fn scaffold_files(name: &str, revision: &str, template: &str) -> Vec<(PathBuf, String)> {
    let package_manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\ndefault-run = \"{name}\"\n\n[package.metadata.rustflow]\ntemplate = \"server\"\n\n[[bin]]\nname = \"migrate\"\npath = \"src/bin/migrate.rs\"\n\n[dependencies]\nrustflow = {{ git = \"{REPOSITORY_URL}\", rev = \"{revision}\" }}\nrustflow-server = {{ git = \"{REPOSITORY_URL}\", rev = \"{revision}\" }}\ntokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\", \"net\"] }}\n"
    );
    let mut files = vec![
        (PathBuf::from("Cargo.toml"), package_manifest),
        (
            PathBuf::from(".gitignore"),
            "/target/\n.env\n.env.*\n!.env.example\n.DS_Store\n".to_owned(),
        ),
        (
            PathBuf::from(".env.example"),
            "# Safe local defaults only; never put secrets in this file.\n# RUSTFLOW_DATABASE_URL accepts sqlite:, postgres://, postgresql://, or mysql:// (MySQL/MariaDB).\n# SQL placeholders and migrations are engine-specific; changing this URL alone does not port SQLite SQL.\n# MongoDB uses a separate MongoConfig/MongoDatabase adapter and mongodb:// URI, not SQLx or SQL migrations.\nRUSTFLOW_ENV=development\nRUSTFLOW_HOST=127.0.0.1\nRUSTFLOW_PORT=3000\nRUSTFLOW_DATABASE_URL=sqlite://rustflow.db?mode=rwc\nRUSTFLOW_DATABASE_MAX_CONNECTIONS=10\nRUSTFLOW_DATABASE_MIN_CONNECTIONS=0\nRUSTFLOW_DATABASE_ACQUIRE_TIMEOUT_SECS=10\n".to_owned(),
        ),
        (
            PathBuf::from("README.md"),
            format!(
                "# {name}\n\nAplicación de servidor generada con RustFlow. Las APIs son pre-1.0 y pueden cambiar.\n\n## Desarrollo\n\n```sh\nrustflow dev\n```\n\nLa configuración inicial es `127.0.0.1:3000`. Copia `.env.example` a `.env` si deseas cambiar los ajustes locales; consulta `docs/CONFIGURATION.md` para precedencia y perfiles. `rustflow dev` ejecuta Cargo una vez; no incluye supervisión de archivos ni hot reload.\n\n## Build\n\n```sh\nrustflow build --release\n```\n\n## Generadores\n\n```sh\nrustflow generate component ProductCard\nrustflow generate page about\nrustflow generate controller health\nrustflow generate model User\nrustflow generate migration create_users\n```\n\nLos generadores de Rust registran el módulo dentro de la sección gestionada de `src/**/mod.rs`. Si el archivo de índice no conserva sus marcadores, el comando se detiene sin reemplazarlo. Las migraciones son archivos SQL que el ejecutor SQLx puede aplicar de forma explícita; no se ejecutan por el comando generate.\n\nLas dependencias de RustFlow están fijadas a la revisión del repositorio desde la que se compiló este CLI. Para actualizar esa revisión, cambia deliberadamente las dependencias de Git y vuelve a probar el proyecto.\n\nLa aplicación usa un pool SQLx configurable y el scaffold empieza con SQLite. SQLx también acepta `postgres://`/`postgresql://` y `mysql://` (driver compartido por MySQL y MariaDB), pero cambiar `RUSTFLOW_DATABASE_URL` no hace portables los placeholders, tipos ni migraciones generados para SQLite: adáptalos al motor y usa directorios de migración específicos. La matriz verificada y los límites están en [DATABASE_SUPPORT_MATRIX.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/DATABASE_SUPPORT_MATRIX.md). MongoDB usa una API documental independiente (`MongoConfig`, `MongoDatabase`, BSON y colecciones); no es seleccionada por `RUSTFLOW_DATABASE_URL`, no usa migraciones SQL y esta plantilla no la inicializa automáticamente. Esta revisión soporta URI `mongodb://`, no `mongodb+srv://`. En desarrollo/test el scaffold aplica migraciones al iniciar; en producción ejecuta `cargo run --bin migrate` como paso de despliegue antes de arrancar varias instancias. Consulta docs/CONFIGURATION.md para precedencia y límites del pool.\n"
            ),
        ),
        (
            PathBuf::from("src/main.rs"),
            r#"use std::{error::Error, sync::Arc};
use rustflow_server::{
    AppConfig, AppEnvironment, Application, DatabaseConfig, DatabaseError, DatabasePool,
    Request, Response,
};

pub mod components;
pub mod controllers;
pub mod models;
pub mod pages;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load()?;
    let address = config.socket_addr();
    let database_config = DatabaseConfig::from_app_config(&config)?;
    let database = Arc::new(DatabasePool::connect(&database_config).await?);
    if config.environment != AppEnvironment::Production {
        database.run_migrations("migrations").await?;
    }

    let database_probe = Arc::clone(&database);
    let mut app = Application::new();
    app.get("/health/database", move |_request: Request| {
        let database = Arc::clone(&database_probe);
        async move {
            database.query_raw("SELECT 1", [])
                .await
                .map_err(|error| error.to_http_error())?;
            Ok(Response::text("ok"))
        }
    })?;
    app.get("/", controllers::home_controller::index)?;
    println!("RustFlow server listening on http://{address}");
    app.serve(address).await?;
    Ok(())
}
"#
            .to_owned(),
        ),
        (
            PathBuf::from("src/components/mod.rs"),
            module_index("Componentes reutilizables."),
        ),
        (
            PathBuf::from("src/pages/mod.rs"),
            module_index_with_modules("Páginas estructurales de la aplicación.", &["home"]),
        ),
        (
            PathBuf::from("src/pages/home.rs"),
            "use rustflow::view::{View, ViewError};\n\n/// Construye la página inicial como una vista estructural.\npub fn render() -> Result<View, ViewError> {\n    View::element(\n        \"main\",\n        [\n            View::element(\"h1\", [View::text(\"Hola, RustFlow\")])?,\n            View::element(\n                \"p\",\n                [View::text(\"Edita src/pages/home.rs para cambiar esta página.\")],\n            )?,\n        ],\n    )\n}\n".to_owned(),
        ),
        (
            PathBuf::from("src/controllers/mod.rs"),
            module_index_with_modules("Controladores HTTP.", &["home_controller"]),
        ),
        (
            PathBuf::from("src/controllers/home_controller.rs"),
            "use rustflow_server::{html_document, HttpError, Request, Response};\n\n/// Responde con la página inicial renderizada en el servidor.\npub async fn index(_request: Request) -> Result<Response, HttpError> {\n    let page = crate::pages::home::render()\n        .map_err(|_| HttpError::internal_server_error())?;\n    html_document(\"RustFlow\", \"es\", &page)\n}\n".to_owned(),
        ),
        (
            PathBuf::from("src/models/mod.rs"),
            module_index("Tipos de dominio de la aplicación; sin persistencia integrada."),
        ),
        (
            PathBuf::from("migrations/.gitkeep"),
            String::new(),
        ),
        (
            PathBuf::from("src/bin/migrate.rs"),
            r#"use std::error::Error;
use rustflow_server::{AppConfig, DatabaseConfig, DatabasePool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load()?;
    let database_config = DatabaseConfig::from_app_config(&config)?;
    let database = DatabasePool::connect(&database_config).await?;
    database.run_migrations("migrations").await?;
    database.close().await;
    println!("RustFlow database migrations completed.");
    Ok(())
}
"#
            .to_owned(),
        ),
    ];
    if template == "crud" {
        for (path, contents) in crud_scaffold_files(name, revision) {
            if let Some((_, existing)) = files.iter_mut().find(|(existing_path, _)| existing_path == &path) {
                *existing = contents;
            } else {
                files.push((path, contents));
            }
        }
    }
    files
}
 
fn crud_scaffold_files(name: &str, revision: &str) -> Vec<(PathBuf, String)> {
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.85\"\ndefault-run = \"{name}\"\n\n[package.metadata.rustflow]\ntemplate = \"crud\"\n\n[[bin]]\nname = \"migrate\"\npath = \"src/bin/migrate.rs\"\n\n[dependencies]\nrustflow = {{ git = \"{REPOSITORY_URL}\", rev = \"{revision}\" }}\nrustflow-server = {{ git = \"{REPOSITORY_URL}\", rev = \"{revision}\" }}\ntokio = {{ version = \"1\", features = [\"macros\", \"rt-multi-thread\", \"net\"] }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nhttp = \"1\"\n"
    );
    vec![
        (PathBuf::from("Cargo.toml"), manifest),
        (
            PathBuf::from("README.md"),
            include_str!("../templates/crud/README.md").replace("{{project_name}}", name),
        ),
        (
            PathBuf::from(".env.example"),
            include_str!("../templates/crud/.env.example").to_owned(),
        ),
        (
            PathBuf::from("src/main.rs"),
            include_str!("../templates/crud/main.rs").to_owned(),
        ),
        (
            PathBuf::from("src/bin/migrate.rs"),
            include_str!("../templates/crud/migrate.rs").to_owned(),
        ),
        (
            PathBuf::from("src/models/mod.rs"),
            module_index_with_modules("Entidades SQL del ejemplo CRUD.", &["product"]),
        ),
        (
            PathBuf::from("src/models/product.rs"),
            include_str!("../templates/crud/product.rs").to_owned(),
        ),
        (
            PathBuf::from("migrations/sqlite/.rustflow-dialect"),
            include_str!("../templates/crud/sqlite/.rustflow-dialect").to_owned(),
        ),
        (
            PathBuf::from("migrations/sqlite/20261031010001_create_products.sql"),
            include_str!("../templates/crud/sqlite/20261031010001_create_products.sql").to_owned(),
        ),
        (
            PathBuf::from("migrations/sqlite/20261031010001_create_products.down.sql"),
            include_str!("../templates/crud/sqlite/20261031010001_create_products.down.sql").to_owned(),
        ),
        (
            PathBuf::from("migrations/postgres/.rustflow-dialect"),
            include_str!("../templates/crud/postgres/.rustflow-dialect").to_owned(),
        ),
        (
            PathBuf::from("migrations/postgres/20261031010002_create_products.sql"),
            include_str!("../templates/crud/postgres/20261031010002_create_products.sql").to_owned(),
        ),
        (
            PathBuf::from("migrations/postgres/20261031010002_create_products.down.sql"),
            include_str!("../templates/crud/postgres/20261031010002_create_products.down.sql").to_owned(),
        ),
        (
            PathBuf::from("migrations/mysql/.rustflow-dialect"),
            include_str!("../templates/crud/mysql/.rustflow-dialect").to_owned(),
        ),
        (
            PathBuf::from("migrations/mysql/20261031010003_create_products.sql"),
            include_str!("../templates/crud/mysql/20261031010003_create_products.sql").to_owned(),
        ),
        (
            PathBuf::from("migrations/mysql/20261031010003_create_products.down.sql"),
            include_str!("../templates/crud/mysql/20261031010003_create_products.down.sql").to_owned(),
        ),
    ]
}

fn module_index(comment: &str) -> String {
    module_index_with_modules(comment, &[])
}

fn module_index_with_modules(comment: &str, initial_modules: &[&str]) -> String {
    let header = if comment.is_empty() {
        String::new()
    } else {
        format!("//! {comment}\n")
    };
    let declarations = initial_modules
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| format!("pub mod {name};"))
        .collect::<Vec<_>>()
        .join("\n");
    if declarations.is_empty() {
        format!("{header}\n{MODULE_START}\n{MODULE_END}\n")
    } else {
        format!("{header}\n{MODULE_START}\n{declarations}\n{MODULE_END}\n")
    }
}

fn find_rustflow_project(start: &Path) -> Result<PathBuf, CliError> {
    for ancestor in start.ancestors() {
        let manifest = ancestor.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&manifest)
            .map_err(|error| format!("could not read {}: {error}", manifest.display()))?;
        if contents.contains("[package]")
            && (contents.contains("[package.metadata.rustflow]")
                || contents.contains("rustflow-server")
                || contents.contains("rustflow ="))
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(format!(
        "no RustFlow application was found from {}; run this command inside a project created with `rustflow new` or one whose Cargo.toml directly depends on RustFlow",
        start.display()
    )
    .into())
}

fn run_cargo(subcommand: &str, args: &[OsString], operation: &str) -> Result<i32, CliError> {
    let current = env::current_dir()
        .map_err(|error| format!("could not determine the current directory: {error}"))?;
    let root = find_rustflow_project(&current)?;
    println!("Running `cargo {subcommand}` in {}", root.display());

    let cargo = env::var_os("RUSTFLOW_CARGO")
        .or_else(|| env::var_os("CARGO"))
        .unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(&cargo)
        .current_dir(&root)
        .arg(subcommand)
        .args(args)
        .status()
        .map_err(|error| {
            format!(
                "could not start Cargo for `rustflow {operation}` ({}): {error}",
                cargo.to_string_lossy()
            )
        })?;
    Ok(status.code().unwrap_or(1))
}

#[derive(Debug)]
struct ModuleIndexUpdate {
    path: PathBuf,
    original: String,
    updated: String,
}

fn generate_file(root: &Path, kind: &str, raw_name: &str) -> Result<PathBuf, CliError> {
    validate_project_root(root)?;
    let kind = match kind {
        "component" | "components" => "component",
        "page" | "pages" => "page",
        "controller" | "controllers" => "controller",
        "model" | "models" => "model",
        "migration" | "migrations" => "migration",
        _ => return Err(format!("unknown generator type `{kind}`").into()),
    };
    let name = generated_identifier(raw_name)?;

    let (relative, content, module) = match kind {
        "component" => (
            PathBuf::from(format!("src/components/{name}.rs")),
            component_template(&name),
            Some(("components", name.clone())),
        ),
        "page" => (
            PathBuf::from(format!("src/pages/{name}.rs")),
            page_template(&name),
            Some(("pages", name.clone())),
        ),
        "controller" => {
            let module = if name.ends_with("_controller") {
                name.clone()
            } else {
                format!("{name}_controller")
            };
            (
                PathBuf::from(format!("src/controllers/{module}.rs")),
                controller_template(&module),
                Some(("controllers", module)),
            )
        }
        "model" => (
            PathBuf::from(format!("src/models/{name}.rs")),
            model_template(&name),
            Some(("models", name.clone())),
        ),
        "migration" => {
            let relative = migration_path(root, &name)?;
            (relative, migration_template(&name), None)
        }
        _ => unreachable!("generator type was normalized above"),
    };

    let target = root.join(&relative);
    if path_exists(&target)? {
        return Err(format!(
            "refusing to overwrite {}; move or rename the existing file/directory before retrying",
            target.display()
        )
        .into());
    }

    let module_update = if let Some((directory, module_name)) = module {
        let index_path = root.join("src").join(directory).join("mod.rs");
        Some(prepare_module_index(&index_path, &module_name)?)
    } else {
        None
    };

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    write_one_file(&target, &content)?;

    if let Some(update) = module_update {
        if let Err(error) = apply_module_index_update(&update) {
            let _ = fs::remove_file(&target);
            return Err(error);
        }
    }

    Ok(target)
}

fn validate_project_root(root: &Path) -> Result<(), CliError> {
    let manifest = root.join("Cargo.toml");
    let contents = fs::read_to_string(&manifest)
        .map_err(|error| format!("could not read {}: {error}", manifest.display()))?;
    if !contents.contains("[package]")
        || !(contents.contains("[package.metadata.rustflow]")
            || contents.contains("rustflow-server")
            || contents.contains("rustflow ="))
    {
        return Err(format!(
            "{} is not a RustFlow application manifest; run `rustflow new` first or add a direct RustFlow dependency",
            manifest.display()
        )
        .into());
    }
    Ok(())
}

fn prepare_module_index(path: &Path, new_module: &str) -> Result<ModuleIndexUpdate, CliError> {
    let original = fs::read_to_string(path)
        .map_err(|error| format!("could not read module index {}: {error}", path.display()))?;
    let lines = original.split_inclusive('\n').collect::<Vec<_>>();
    let mut offset = 0;
    let mut start_after = None;
    let mut end_start = None;

    for line in &lines {
        let clean = line.trim_end_matches(['\r', '\n']);
        if clean == MODULE_START {
            if start_after.is_some() {
                return Err(format!(
                    "module index {} contains duplicate managed-section markers",
                    path.display()
                )
                .into());
            }
            start_after = Some(offset + line.len());
        } else if clean == MODULE_END {
            if end_start.is_some() {
                return Err(format!(
                    "module index {} contains duplicate managed-section markers",
                    path.display()
                )
                .into());
            }
            end_start = Some(offset);
        }
        offset += line.len();
    }

    let (Some(start_after), Some(end_start)) = (start_after, end_start) else {
        return Err(format!(
            "module index {} no longer contains the RustFlow managed-module markers; restore {MODULE_START} and {MODULE_END}, then retry. The file was not changed.",
            path.display()
        ).into());
    };
    if end_start < start_after {
        return Err(format!(
            "module index {} has reversed managed-section markers",
            path.display()
        )
        .into());
    }

    let managed = &original[start_after..end_start];
    let mut modules = BTreeSet::new();
    for line in managed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(name) = line
            .strip_prefix("pub mod ")
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            return Err(format!(
                "module index {} contains user content inside its managed section; move custom lines outside the markers to let the generator update it safely",
                path.display()
            ).into());
        };
        if !is_valid_module_token(name) {
            return Err(format!("invalid module declaration in {}", path.display()).into());
        }
        modules.insert(name.to_owned());
    }

    if modules.contains(new_module) {
        return Err(format!(
            "module index {} already declares `{new_module}` but its generated file is absent; reconcile the module manually before retrying",
            path.display()
        ).into());
    }
    let declaration_outside_managed = original
        .split(MODULE_START)
        .next()
        .unwrap_or("")
        .lines()
        .chain(original.split(MODULE_END).nth(1).unwrap_or("").lines())
        .any(|line| line.trim() == format!("pub mod {new_module};"));
    if declaration_outside_managed {
        return Err(format!(
            "module `{new_module}` is already declared outside the managed section of {}; reconcile it manually before retrying",
            path.display()
        ).into());
    }

    modules.insert(new_module.to_owned());
    let declarations = modules
        .into_iter()
        .map(|name| format!("pub mod {name};"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut updated = String::new();
    updated.push_str(&original[..start_after]);
    updated.push_str(&declarations);
    if !declarations.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&original[end_start..]);

    Ok(ModuleIndexUpdate {
        path: path.to_path_buf(),
        original,
        updated,
    })
}

fn is_valid_module_token(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !is_rust_keyword(name)
}

fn apply_module_index_update(update: &ModuleIndexUpdate) -> Result<(), CliError> {
    let current = fs::read_to_string(&update.path)
        .map_err(|error| format!("could not re-read {}: {error}", update.path.display()))?;
    if current != update.original {
        return Err(format!(
            "module index {} changed while generation was in progress; the generated source was removed, inspect your changes and retry",
            update.path.display()
        )
        .into());
    }

    let parent = update
        .path
        .parent()
        .ok_or_else(|| CliError("module index has no parent directory".to_owned()))?;
    let file_name = update
        .path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("mod.rs");
    let temporary = parent.join(format!(
        ".{file_name}.rustflow-tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(update.updated.as_bytes())?;
        file.flush()?;
        fs::rename(&temporary, &update.path)?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not update module index {} atomically: {error}; generated source was removed",
            update.path.display()
        )
        .into());
    }
    Ok(())
}

fn write_one_file(path: &Path, contents: &str) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "refusing to overwrite existing destination {}",
                    path.display()
                )
            } else {
                format!("could not create {}: {error}", path.display())
            }
        })?;
    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.flush())
    {
        let _ = fs::remove_file(path);
        return Err(format!("could not finish writing {}: {error}", path.display()).into());
    }
    Ok(())
}

fn migration_path(root: &Path, name: &str) -> Result<PathBuf, CliError> {
    let directory = root.join("migrations");
    let mut entries = fs::read_dir(&directory)
        .map_err(|error| {
            format!(
                "could not inspect migrations directory {}: {error}",
                directory.display()
            )
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(|error| CliError(format!("could not inspect migration entry: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    let expected_name = format!("{name}.sql");
    let duplicate_exists = entries.iter().any(|entry| {
        let Some((sequence, migration_name)) = entry.split_once('_') else {
            return false;
        };
        sequence.len() == 4
            && sequence.bytes().all(|byte| byte.is_ascii_digit())
            && migration_name == expected_name
    });
    if duplicate_exists {
        return Err(format!(
            "a migration named `{name}` already exists; choose a different migration name"
        )
        .into());
    }

    let highest = entries
        .iter()
        .filter_map(|entry| {
            let (prefix, _) = entry.split_once('_')?;
            if prefix.len() != 4 || !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            prefix.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);

    let next = highest
        .checked_add(1)
        .ok_or_else(|| CliError("migration sequence number overflow".to_owned()))?;
    Ok(PathBuf::from(format!("migrations/{next:04}_{name}.sql")))
}

fn component_template(name: &str) -> String {
    let title = humanize(name);
    format!(
        "use rustflow::view::{{View, ViewError}};\n\n/// Renderiza el componente {title}.\npub fn render(title: &str) -> Result<View, ViewError> {{\n    View::element(\n        \"section\",\n        [\n            View::element(\"h2\", [View::text(title)])?,\n            View::element(\"p\", [View::text(\"Componente generado por RustFlow.\")])?,\n        ],\n    )\n}}\n"
    )
}

fn page_template(name: &str) -> String {
    let title = humanize(name);
    format!(
        "use rustflow::view::{{View, ViewError}};\n\n/// Construye la página {title} como una vista estructural.\npub fn render() -> Result<View, ViewError> {{\n    View::element(\n        \"main\",\n        [\n            View::element(\"h1\", [View::text(\"{title}\")])?,\n            View::element(\"p\", [View::text(\"Edita esta página para añadir contenido.\")])?,\n        ],\n    )\n}}\n"
    )
}

fn controller_template(module: &str) -> String {
    let title = humanize(module.trim_end_matches("_controller"));
    format!(
        "use rustflow_server::{{HttpError, Request, Response}};\n\n/// Handler HTTP de {title}.\npub async fn index(_request: Request) -> Result<Response, HttpError> {{\n    Ok(Response::text(\"Controlador {title} generado por RustFlow\"))\n}}\n"
    )
}

fn model_template(name: &str) -> String {
    let type_name = name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    format!(
        "/// Tipo de dominio generado; no representa un modelo ORM.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {type_name} {{\n    pub id: i64,\n    pub name: String,\n}}\n"
    )
}

fn migration_template(name: &str) -> String {
    let table = name.strip_prefix("create_").unwrap_or(name);
    format!(
        "-- Plantilla SQL generada por RustFlow.\n-- Se aplica solo cuando la app ejecuta DatabasePool::run_migrations.\n-- Es irreversible porque no tiene archivo .down.sql; en producción prefiere una migración correctiva hacia delante.\n-- Revisa el dialecto del motor antes de aplicar.\n\nCREATE TABLE IF NOT EXISTS {table} (\n    id INTEGER PRIMARY KEY,\n    name TEXT NOT NULL\n);\n"
    )
}

fn humanize(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_names_reject_paths_and_invalid_identifiers() {
        assert!(validate_project_name("notes-app").is_ok());
        assert!(validate_project_name("notes_app2").is_ok());
        assert!(validate_project_name("../notes").is_err());
        assert!(validate_project_name("9notes").is_err());
        assert!(validate_project_name("notes/app").is_err());
    }

    #[test]
    fn generated_names_are_normalized_and_keywords_rejected() {
        assert_eq!(generated_identifier("ProductCard").unwrap(), "product_card");
        assert_eq!(
            generated_identifier("user-profile").unwrap(),
            "user_profile"
        );
        assert!(generated_identifier("type").is_err());
        assert!(generated_identifier("bad/name").is_err());
    }

    #[test]
    fn failed_staging_population_removes_partial_files() {
        let parent = temporary_directory("staging-cleanup");
        let staging = parent.join("staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("blocked"), "not a directory").unwrap();
        let files = vec![
            (PathBuf::from("first.txt"), "partial content".to_owned()),
            (
                PathBuf::from("blocked/second.txt"),
                "this write must fail".to_owned(),
            ),
        ];

        let error = populate_staging_dir(&staging, &files).unwrap_err();
        assert!(error.to_string().contains("could not create directory"));
        assert!(!staging.exists());
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn migration_sequence_is_derived_from_sorted_existing_names() {
        let root = temporary_directory("migration-sequence");
        fs::create_dir_all(root.join("migrations")).unwrap();
        fs::write(root.join("migrations/.gitkeep"), "").unwrap();
        fs::write(root.join("migrations/0004_existing.sql"), "-- existing").unwrap();
        fs::write(root.join("migrations/0002_old.sql"), "-- existing").unwrap();
        fs::write(root.join("migrations/0003_my_user.sql"), "-- existing").unwrap();

        assert_eq!(
            migration_path(&root, "create_users").unwrap(),
            PathBuf::from("migrations/0005_create_users.sql")
        );
        assert_eq!(
            migration_path(&root, "user").unwrap(),
            PathBuf::from("migrations/0005_user.sql")
        );
        assert!(migration_path(&root, "old").is_err());
        assert!(migration_path(&root, "existing").is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_module_entries_are_sorted_and_outside_content_is_preserved() {
        let root = temporary_directory("module-index");
        fs::create_dir_all(root.join("src/components")).unwrap();
        let index = root.join("src/components/mod.rs");
        let original = format!(
            "//! User documentation stays above the managed section.\n{MODULE_START}\n{MODULE_END}\n// User notes stay below.\n"
        );
        fs::write(&index, &original).unwrap();

        assert!(
            !fs::read_to_string(&index)
                .unwrap()
                .contains("pub mod home;")
        );
        let first = prepare_module_index(&index, "zebra").unwrap();
        write_one_file(&root.join("src/components/zebra.rs"), "pub fn zebra() {}\n").unwrap();
        apply_module_index_update(&first).unwrap();
        let second = prepare_module_index(&index, "alpha").unwrap();
        write_one_file(&root.join("src/components/alpha.rs"), "pub fn alpha() {}\n").unwrap();
        apply_module_index_update(&second).unwrap();

        let output = fs::read_to_string(&index).unwrap();
        assert!(output.find("pub mod alpha;").unwrap() < output.find("pub mod zebra;").unwrap());
        assert!(output.contains("// User notes stay below."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_or_unmanaged_module_index_is_not_replaced() {
        let root = temporary_directory("module-index-conflict");
        fs::create_dir_all(root.join("src/components")).unwrap();
        let index = root.join("src/components/mod.rs");
        fs::write(&index, "pub mod custom;\n").unwrap();
        assert!(prepare_module_index(&index, "new_module").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "rustflow-cli-unit-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
