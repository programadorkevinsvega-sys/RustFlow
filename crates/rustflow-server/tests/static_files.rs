use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use http::{Method, StatusCode};
use rustflow_server::{Application, StaticFiles, testing::TestApp};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn temp_directory(label: &str) -> std::path::PathBuf {
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "rustflow-static-files-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temporary static root");
    path
}

fn write_file(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create static parent");
    }
    fs::write(path, contents).expect("write static file");
}

#[test]
fn static_files_resolve_mime_cache_limit_and_index() {
    let root = temp_directory("resolve");
    write_file(&root, "index.html", b"<h1>ok</h1>");
    write_file(&root, "app.css", b"body{}");
    write_file(&root, "private.bin", &[b'x'; 101]);

    let files = StaticFiles::new(&root)
        .unwrap()
        .with_max_file_bytes(100)
        .unwrap()
        .with_cache_control("public, max-age=120")
        .unwrap()
        .with_index_file("index.html")
        .unwrap();

    let index = files.resolve("").expect("index");
    assert_eq!(index.content_type(), "text/html; charset=utf-8");
    assert_eq!(index.cache_control(), "public, max-age=120");

    let css = files.resolve("app.css").unwrap();
    assert_eq!(css.content_type(), "text/css; charset=utf-8");
    assert!(files.resolve("private.bin").is_none());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn static_files_reject_traversal_absolute_dotfiles_and_symlinks() {
    let root = temp_directory("containment");
    write_file(&root, "safe.txt", b"safe");
    write_file(&root, ".secret", b"secret");
    fs::write(root.join("outside.txt"), b"outside").unwrap();

    let files = StaticFiles::new(&root).unwrap();
    assert!(files.resolve("../outside.txt").is_none());
    assert!(files.resolve("/absolute.txt").is_none());
    assert!(files.resolve(".secret").is_none());
    assert!(files.resolve("%2e%2e/outside.txt").is_none());
    assert!(files.resolve("missing.txt").is_none());
    assert_eq!(files.resolve("safe.txt").unwrap().body(), b"safe");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("safe.txt"), root.join("linked.txt")).unwrap();
        assert!(files.resolve("linked.txt").is_none());

        let outside = temp_directory("symlink-outside");
        write_file(&outside, "out.txt", b"outside");
        std::os::unix::fs::symlink(&outside, root.join("linked-dir")).unwrap();
        assert!(files.resolve("linked-dir/out.txt").is_none());
        fs::remove_dir_all(outside).unwrap();
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn static_files_reject_invalid_configuration() {
    assert!(matches!(
        StaticFiles::new("/definitely/missing/rustflow-static-root"),
        Err(rustflow_server::ServerError::StaticRootNotFound)
    ));
    let root = temp_directory("config");
    let file = root.join("single-file");
    fs::write(&file, b"x").unwrap();
    assert!(matches!(
        StaticFiles::new(&file),
        Err(rustflow_server::ServerError::StaticRootNotDirectory)
    ));
    assert!(
        StaticFiles::new(&root)
            .unwrap()
            .with_max_file_bytes(0)
            .is_err()
    );
    assert!(
        StaticFiles::new(&root)
            .unwrap()
            .with_cache_control("\n")
            .is_err()
    );
    assert!(
        StaticFiles::new(&root)
            .unwrap()
            .with_index_file("/index.html")
            .is_err()
    );

    let mut app = Application::new();
    let files = StaticFiles::new(&root).unwrap();
    assert!(matches!(
        app.static_files("assets", files),
        Err(rustflow_server::ServerError::InvalidStaticPrefix)
    ));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_get_head_and_missing() {
    let root = temp_directory("http");
    write_file(&root, "app.css", b"body{}");
    write_file(&root, "index.html", b"<h1>index</h1>");
    let files = StaticFiles::new(&root)
        .unwrap()
        .with_index_file("index.html")
        .unwrap();

    let mut app = Application::new();
    app.static_files("/assets", files.clone())
        .expect("mount assets");
    app.static_files("/", files).expect("mount index");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    let css = app.get("/assets/app.css").send().await.unwrap();
    assert_eq!(css.status(), StatusCode::OK);
    assert_eq!(
        css.headers()[http::header::CONTENT_TYPE],
        "text/css; charset=utf-8"
    );
    assert_eq!(css.text().unwrap(), "body{}");

    let head = app
        .request(Method::HEAD, "/assets/app.css")
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert!(head.body().is_empty());
    assert_eq!(
        head.headers()[http::header::CONTENT_TYPE],
        "text/css; charset=utf-8"
    );
    assert_eq!(
        head.headers()[http::header::CACHE_CONTROL],
        "public, max-age=3600"
    );

    let missing_head = app
        .request(Method::HEAD, "/assets/missing.txt")
        .send()
        .await
        .unwrap();
    assert_eq!(missing_head.status(), StatusCode::NOT_FOUND);
    assert!(missing_head.body().is_empty());

    let missing = app.get("/assets/missing.txt").send().await.unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.body(), b"Static file not found");

    let traversal = app.get("/assets/../index.html").send().await.unwrap();
    assert_eq!(traversal.status(), StatusCode::NOT_FOUND);

    let method = app.post("/assets/app.css").send().await.unwrap();
    assert_eq!(method.status(), StatusCode::METHOD_NOT_ALLOWED);

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_range_conditional_and_spa_fallback() {
    let root = temp_directory("range");
    write_file(&root, "app.css", b"body{}");
    write_file(&root, "app.html", b"<h1>spa</h1>");
    let files = StaticFiles::new(&root)
        .unwrap()
        .with_spa_fallback("app.html")
        .unwrap();

    let mut app = Application::new();
    app.static_files("/assets", files).expect("mount assets");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    let range = app
        .get("/assets/app.css")
        .header("range", "bytes=0-3")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(range.headers()[http::header::CONTENT_RANGE], "bytes 0-3/6");
    assert_eq!(range.headers()[http::header::ACCEPT_RANGES], "bytes");
    assert_eq!(range.body(), b"body");

    let resolved = StaticFiles::new(&root).unwrap();
    let resolved = resolved.resolve("app.css").unwrap();
    let etag = resolved.etag().to_str().unwrap().to_owned();
    let last_modified = resolved.last_modified().to_str().unwrap().to_owned();

    let conditional = app
        .get("/assets/app.css")
        .header("if-none-match", &etag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(conditional.status(), StatusCode::NOT_MODIFIED);
    assert!(conditional.body().is_empty());

    let modified = app
        .get("/assets/app.css")
        .header("if-modified-since", &last_modified)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(modified.status(), StatusCode::NOT_MODIFIED);

    let unsatisfied = app
        .get("/assets/app.css")
        .header("range", "bytes=99-100")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(unsatisfied.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        unsatisfied.headers()[http::header::CONTENT_RANGE],
        "bytes */6"
    );

    let fallback = app.get("/assets/unknown/route").send().await.unwrap();
    assert_eq!(fallback.status(), StatusCode::OK);
    assert_eq!(fallback.text().unwrap(), "<h1>spa</h1>");

    fs::remove_dir_all(root).unwrap();
}
