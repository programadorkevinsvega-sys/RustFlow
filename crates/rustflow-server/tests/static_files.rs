use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use http::{Method, StatusCode};
use rustflow_server::{Application, HttpError, Middleware, StaticFiles, testing::TestApp};

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
    write_file(&root, ".env", b"secret");
    write_file(&root, ".git/config", b"secret");
    write_file(&root, "directory/index.html", b"not a listing");
    fs::write(root.join("outside.txt"), b"outside").unwrap();

    let files = StaticFiles::new(&root).unwrap();
    assert!(files.resolve("../outside.txt").is_none());
    assert!(files.resolve("/absolute.txt").is_none());
    assert!(files.resolve(".secret").is_none());
    assert!(files.resolve(".env").is_none());
    assert!(files.resolve(".git/config").is_none());
    assert!(files.resolve("directory").is_none());
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
    write_file(&root, ".env", b"secret");
    write_file(&root, ".git/config", b"secret");
    write_file(&root, "directory/index.html", b"not a listing");
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

    for path in ["/assets/.env", "/assets/.git/config", "/assets/directory"] {
        let response = app.get(path).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    let traversal = app
        .get("/assets/../index.html")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-none-match", "*")
        .unwrap()
        .send()
        .await
        .unwrap();
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

    let fallback = app
        .get("/assets/unknown/route")
        .header("accept", "text/html")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(fallback.status(), StatusCode::OK);
    assert_eq!(fallback.text().unwrap(), "<h1>spa</h1>");

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_range_edge_cases_and_head() {
    let root = temp_directory("range-edge");
    write_file(&root, "app.css", b"abcdef");
    write_file(&root, "empty.txt", b"");
    let files = StaticFiles::new(&root).unwrap();

    let mut app = Application::new();
    app.static_files("/assets", files).expect("mount assets");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    for (range, start, end, body) in [
        ("bytes=0-0", 0usize, 1usize, b"a".as_slice()),
        ("bytes=-3", 3, 6, b"def".as_slice()),
        ("bytes=2-", 2, 6, b"cdef".as_slice()),
        ("bytes=4-999", 4, 6, b"ef".as_slice()),
    ] {
        let response = app
            .get("/assets/app.css")
            .header("range", range)
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers()[http::header::CONTENT_RANGE],
            format!("bytes {}-{}/6", start, end - 1)
        );
        assert_eq!(response.body(), body);
    }

    for range in [
        "bytes=99-100",
        "bytes=18446744073709551615-18446744073709551615",
    ] {
        let response = app
            .get("/assets/app.css")
            .header("range", range)
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[http::header::CONTENT_RANGE], "bytes */6");
    }

    for range in ["bytes=abc", "bytes=5-2", "bytes=0-1,4-5"] {
        let response = app
            .get("/assets/app.css")
            .header("range", range)
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), b"abcdef");
    }

    for range in [
        "bytes=0-18446744073709551615",
        "bytes=-18446744073709551615",
    ] {
        let response = app
            .get("/assets/app.css")
            .header("range", range)
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), b"abcdef");
    }

    let empty = app
        .get("/assets/empty.txt")
        .header("range", "bytes=0-0")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(empty.headers()[http::header::CONTENT_RANGE], "bytes */0");

    let resolved = StaticFiles::new(&root).unwrap();
    let resolved = resolved.resolve("app.css").unwrap();
    let etag = resolved.etag().to_str().unwrap().to_owned();
    let last_modified = resolved.last_modified().to_str().unwrap().to_owned();

    let head = app
        .request(Method::HEAD, "/assets/app.css")
        .header("range", "bytes=1-3")
        .unwrap()
        .send()
        .await
        .unwrap();
    let get_range = app
        .get("/assets/app.css")
        .header("range", "bytes=1-3")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(head.status(), get_range.status());
    for header in [
        http::header::CONTENT_TYPE,
        http::header::CONTENT_RANGE,
        http::header::CONTENT_LENGTH,
        http::header::CACHE_CONTROL,
        http::header::ETAG,
        http::header::LAST_MODIFIED,
        http::header::ACCEPT_RANGES,
    ] {
        assert_eq!(
            head.headers().get(&header),
            get_range.headers().get(&header)
        );
    }
    assert_eq!(head.headers()[http::header::CONTENT_RANGE], "bytes 1-3/6");
    assert_eq!(head.headers()[http::header::CONTENT_LENGTH], "3");
    assert!(head.body().is_empty());

    let mismatched = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", "\"another-etag\"")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(mismatched.status(), StatusCode::OK);
    assert_eq!(mismatched.body(), b"abcdef");

    let weak_if_range = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", &etag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(weak_if_range.status(), StatusCode::OK);
    assert_eq!(weak_if_range.body(), b"abcdef");

    let matching_if_range_date = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", &last_modified)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(matching_if_range_date.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(matching_if_range_date.body(), b"ab");

    let stale_if_range_date = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", "Wed, 21 Oct 2015 07:28:00 GMT")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(stale_if_range_date.status(), StatusCode::OK);
    assert_eq!(stale_if_range_date.body(), b"abcdef");

    let weak_etag = app
        .get("/assets/app.css")
        .header("if-none-match", &etag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(weak_etag.status(), StatusCode::NOT_MODIFIED);
    assert!(weak_etag.body().is_empty());
    assert!(weak_etag.headers().contains_key(http::header::ETAG));
    assert!(
        weak_etag
            .headers()
            .contains_key(http::header::CACHE_CONTROL)
    );

    let etag_list = app
        .get("/assets/app.css")
        .header("if-none-match", &format!("\"other\", {etag}"))
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(etag_list.status(), StatusCode::NOT_MODIFIED);

    let star = app
        .get("/assets/app.css")
        .header("if-none-match", "*")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(star.status(), StatusCode::NOT_MODIFIED);

    let precedence = app
        .get("/assets/app.css")
        .header("if-none-match", "\"not-the-current-etag\"")
        .unwrap()
        .header("if-modified-since", "Thu, 01 Jan 2037 00:00:00 GMT")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(precedence.status(), StatusCode::OK);

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_head_range_headers_match_get() {
    let root = temp_directory("range-head");
    write_file(&root, "app.css", b"abcdef");
    let mut app = Application::new();
    app.static_files("/assets", StaticFiles::new(&root).unwrap())
        .expect("mount assets");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    let head = app
        .request(Method::HEAD, "/assets/app.css")
        .header("range", "bytes=1-3")
        .unwrap()
        .send()
        .await
        .unwrap();
    let get = app
        .get("/assets/app.css")
        .header("range", "bytes=1-3")
        .unwrap()
        .send()
        .await
        .unwrap();

    assert_eq!(head.status(), get.status());
    assert_eq!(
        head.headers()[http::header::CONTENT_RANGE],
        get.headers()[http::header::CONTENT_RANGE]
    );
    assert_eq!(
        head.headers()[http::header::CONTENT_LENGTH],
        get.headers()[http::header::CONTENT_LENGTH]
    );
    assert!(head.body().is_empty());
    assert_eq!(get.body(), b"bcd");

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_range_does_not_buffer_a_large_full_file() {
    let root = temp_directory("range-large");
    let file_path = root.join("large.bin");
    let file = fs::File::create(&file_path).unwrap();
    file.set_len(100 * 1024 * 1024).unwrap();
    drop(file);

    let mut app = Application::new();
    app.static_files(
        "/assets",
        StaticFiles::new(&root)
            .unwrap()
            .with_max_file_bytes(100 * 1024 * 1024)
            .unwrap(),
    )
    .expect("mount assets");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    let response = app
        .get("/assets/large.bin")
        .header("range", "bytes=1048576-1048579")
        .unwrap()
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers()[http::header::CONTENT_RANGE],
        "bytes 1048576-1048579/104857600"
    );
    assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "4");
    assert_eq!(response.body(), &[0u8; 4]);

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_http_if_range_requires_strong_validator() {
    let root = temp_directory("range-if");
    write_file(&root, "app.css", b"abcdef");
    let mut app = Application::new();
    app.static_files("/assets", StaticFiles::new(&root).unwrap())
        .expect("mount assets");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");
    let resolved = StaticFiles::new(&root).unwrap().resolve("app.css").unwrap();
    let weak_etag = resolved.etag().to_str().unwrap().to_owned();
    let last_modified = resolved.last_modified().to_str().unwrap().to_owned();

    let weak = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", &weak_etag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(weak.status(), StatusCode::OK);
    assert_eq!(weak.body(), b"abcdef");

    let matching_date = app
        .get("/assets/app.css")
        .header("range", "bytes=0-1")
        .unwrap()
        .header("if-range", &last_modified)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(matching_date.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(matching_date.body(), b"ab");

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_spa_fallback_accepts_html_only_and_respects_reserved_paths() {
    let root = temp_directory("spa");
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

    let html = app
        .get("/assets/client/route")
        .header("accept", "text/html")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(html.status(), StatusCode::OK);
    assert_eq!(html.text().unwrap(), "<h1>spa</h1>");

    for path in ["/assets/../app.html", "/assets/%2e%2e/app.html"] {
        let traversal = app
            .get(path)
            .header("accept", "text/html")
            .unwrap()
            .header("range", "bytes=0-1")
            .unwrap()
            .header("if-none-match", "*")
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(traversal.status(), StatusCode::NOT_FOUND, "{path}");
    }

    let js = app
        .get("/assets/app.js")
        .header("accept", "text/html")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(js.status(), StatusCode::NOT_FOUND);

    for path in [
        "/assets/api/users",
        "/assets/health/live",
        "/assets/graphql",
    ] {
        let reserved = app
            .get(path)
            .header("accept", "text/html")
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(reserved.status(), StatusCode::NOT_FOUND, "{path}");
    }

    let non_html = app
        .get("/assets/unknown/route")
        .header("accept", "*/*")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(non_html.status(), StatusCode::NOT_FOUND);

    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn static_files_obey_global_authorization_middleware() {
    let root = temp_directory("middleware");
    write_file(&root, "private.txt", b"private");
    let mut app = Application::new();
    app.static_files("/assets", StaticFiles::new(&root).unwrap())
        .expect("mount static files");
    app.middleware(Middleware::new(|request, next| async move {
        if request.header("x-test-auth").is_none() {
            return Err(HttpError::new(StatusCode::UNAUTHORIZED, "Unauthorized"));
        }
        let response = next.run(request).await?;
        Ok(response.with_header(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("private, no-store"),
        ))
    }))
    .expect("register authorization middleware");
    let app = TestApp::builder()
        .application(app)
        .build()
        .expect("test app");

    let unauthorized = app.get("/assets/private.txt").send().await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = app
        .get("/assets/private.txt")
        .header("x-test-auth", "allowed")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(authorized.body(), b"private");
    assert_eq!(
        authorized.headers()[http::header::CACHE_CONTROL],
        "private, no-store"
    );

    let etag = authorized
        .headers()
        .get(http::header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let conditional = app
        .get("/assets/private.txt")
        .header("x-test-auth", "allowed")
        .unwrap()
        .header("if-none-match", &etag)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(conditional.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        conditional.headers()[http::header::CACHE_CONTROL],
        "private, no-store"
    );
    assert!(conditional.body().is_empty());

    let unauthorized_conditional = app
        .get("/assets/private.txt")
        .header("if-none-match", "*")
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized_conditional.status(), StatusCode::UNAUTHORIZED);

    fs::remove_dir_all(root).unwrap();
}
