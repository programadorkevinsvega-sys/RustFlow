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
        "src/main.rs",
        "src/components/mod.rs",
        "src/pages/home.rs",
        "src/controllers/home_controller.rs",
        "src/models/mod.rs",
        "migrations/.gitkeep",
    ] {
        assert!(project.join(path).is_file(), "missing {path}");
    }

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
        .env("RUSTFLOW_PORT", port.to_string())
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
}

#[test]
fn dev_runs_cargo_and_invalid_projects_return_a_clear_error() {
    let temp = TempDir::new("dev-contract");
    let outside = invoke(temp.path(), &["dev"]);
    assert_eq!(outside.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&outside.stderr).contains("no RustFlow application"));

    let project = create_project(temp.path(), "dev-fixture-app");
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
