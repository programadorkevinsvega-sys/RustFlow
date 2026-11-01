use http::{HeaderName, HeaderValue, StatusCode, header::SET_COOKIE};
use rustflow_server::{HttpMethod, Middleware, Request, Response, testing::TestApp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Echo {
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct SearchQuery {
    term: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ContactForm {
    name: String,
}

#[tokio::test]
async fn in_process_json_requests_preserve_headers_cookies_and_csrf_header() {
    let app = TestApp::builder()
        .configure(|application| {
            application.post("/api/echo", |request: Request| async move {
                let payload: Echo = request.json()?;
                let cookie = request
                    .header("cookie")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();
                let csrf = request
                    .header("x-csrf-token")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();

                let response = Response::json(&payload)?
                    .with_header(
                        HeaderName::from_static("x-test-cookie-seen"),
                        HeaderValue::from_bytes(cookie.as_bytes()).map_err(|_| {
                            rustflow_server::HttpError::bad_request("invalid cookie header")
                        })?,
                    )
                    .with_header(
                        HeaderName::from_static("x-test-csrf-seen"),
                        HeaderValue::from_bytes(csrf.as_bytes()).map_err(|_| {
                            rustflow_server::HttpError::bad_request("invalid CSRF header")
                        })?,
                    );
                Ok(response)
            })?;
            Ok(())
        })
        .build()
        .expect("build in-process application");

    let response = app
        .post("/api/echo")
        .accept("application/json")
        .expect("valid Accept")
        .cookie("session", "test-session")
        .expect("valid cookie")
        .csrf_token("test-csrf-token")
        .expect("valid CSRF header")
        .json(&Echo {
            message: "hello RustFlow".to_owned(),
        })
        .expect("serialize JSON")
        .send()
        .await
        .expect("dispatch request");

    assert_eq!(response.status(), StatusCode::OK);
    let payload: Echo = response.json().expect("deserialize JSON response");
    assert_eq!(payload.message, "hello RustFlow");
    assert_eq!(
        response
            .header("x-test-cookie-seen")
            .and_then(|value| value.to_str().ok()),
        Some("session=test-session")
    );
    assert_eq!(
        response
            .header("x-test-csrf-seen")
            .and_then(|value| value.to_str().ok()),
        Some("test-csrf-token")
    );
}

#[tokio::test]
async fn query_and_urlencoded_form_helpers_use_real_request_parsing() {
    let app = TestApp::builder()
        .configure(|application| {
            application.get("/search", |request: Request| async move {
                let query: SearchQuery = request.query()?;
                Ok(Response::text(query.term))
            })?;
            application.post("/contact", |request: Request| async move {
                let form: ContactForm = request.form()?;
                Ok(Response::text(form.name))
            })?;
            Ok(())
        })
        .build()
        .expect("build in-process application");

    let search = app
        .get("/search")
        .query(&SearchQuery {
            term: "rust web".to_owned(),
        })
        .expect("serialize query")
        .send()
        .await
        .expect("dispatch query");
    assert_eq!(search.status(), StatusCode::OK);
    assert_eq!(search.text().expect("UTF-8 response"), "rust web");

    let form = app
        .post("/contact")
        .form_urlencoded(&ContactForm {
            name: "Ada Lovelace".to_owned(),
        })
        .expect("serialize form")
        .send()
        .await
        .expect("dispatch form");
    assert_eq!(form.status(), StatusCode::OK);
    assert_eq!(form.text().expect("UTF-8 response"), "Ada Lovelace");
}

#[tokio::test]
async fn application_middleware_and_redirects_are_preserved() {
    let app = TestApp::builder()
        .configure(|application| {
            application.middleware(Middleware::new(|request, next| async move {
                let response = next.run(request).await?;
                Ok(response.with_header(
                    HeaderName::from_static("x-test-middleware"),
                    HeaderValue::from_static("executed"),
                ))
            }))?;
            application.get("/redirect", |_request: Request| async {
                Response::redirect("/login")
            })?;
            Ok(())
        })
        .build()
        .expect("build in-process application");

    let response = app
        .get("/redirect")
        .send()
        .await
        .expect("dispatch redirect");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.location().and_then(|value| value.to_str().ok()),
        Some("/login")
    );
    assert_eq!(
        response
            .header("x-test-middleware")
            .and_then(|value| value.to_str().ok()),
        Some("executed")
    );
}

#[tokio::test]
async fn request_helpers_cover_get_post_put_patch_and_delete() {
    let app = TestApp::builder()
        .configure(|application| {
            application.route(
                HttpMethod::Get,
                "/methods/get",
                |request: Request| async move { Ok(Response::text(request.method().as_str())) },
            )?;
            application.route(
                HttpMethod::Post,
                "/methods/post",
                |request: Request| async move { Ok(Response::text(request.method().as_str())) },
            )?;
            application.route(
                HttpMethod::Put,
                "/methods/put",
                |request: Request| async move { Ok(Response::text(request.method().as_str())) },
            )?;
            application.route(
                HttpMethod::Patch,
                "/methods/patch",
                |request: Request| async move { Ok(Response::text(request.method().as_str())) },
            )?;
            application.route(
                HttpMethod::Delete,
                "/methods/delete",
                |request: Request| async move { Ok(Response::text(request.method().as_str())) },
            )?;
            Ok(())
        })
        .build()
        .expect("build in-process application");

    let cases = [
        (app.get("/methods/get"), "GET"),
        (app.post("/methods/post"), "POST"),
        (app.put("/methods/put"), "PUT"),
        (app.patch("/methods/patch"), "PATCH"),
        (app.delete("/methods/delete"), "DELETE"),
    ];
    for (request, expected) in cases {
        let response = request.send().await.expect("dispatch method");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().expect("UTF-8 method"), expected);
    }
}

#[tokio::test]
async fn response_helpers_expose_multiple_set_cookie_headers() {
    let app = TestApp::builder()
        .configure(|application| {
            application.get("/cookies", |_request: Request| async {
                Ok(Response::status(StatusCode::NO_CONTENT)
                    .with_appended_header(
                        SET_COOKIE,
                        HeaderValue::from_static("first=one; Path=/; HttpOnly"),
                    )
                    .with_appended_header(
                        SET_COOKIE,
                        HeaderValue::from_static("second=two; Path=/; HttpOnly"),
                    ))
            })?;
            Ok(())
        })
        .build()
        .expect("build in-process application");

    let response = app
        .get("/cookies")
        .send()
        .await
        .expect("dispatch cookie response");
    let cookies = response
        .set_cookie_headers()
        .map(|cookie| cookie.to_str().expect("valid Set-Cookie").to_owned())
        .collect::<Vec<_>>();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(cookies.len(), 2);
    assert!(cookies.iter().any(|cookie| cookie.starts_with("first=one")));
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with("second=two"))
    );
}
