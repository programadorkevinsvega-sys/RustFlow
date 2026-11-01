use std::{error::Error, net::SocketAddr, sync::Arc, time::Duration};

use http::StatusCode;
use rustflow_server::{
    AccessPolicy, Application, AuthIdentity, CsrfProtection, HttpError, HttpMethod, PasswordHash,
    Request, Response, RolePolicy, SessionCookieConfig, SessionManager,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CsrfPayload {
    token: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct LoginPayload {
    authenticated: bool,
    subject: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct WhoAmIPayload {
    subject: String,
}

fn build_application(
    username: String,
    password_hash: PasswordHash,
    csrf: CsrfProtection,
) -> Result<Application, Box<dyn Error>> {
    let sessions = SessionManager::in_memory(Duration::from_secs(30 * 60))?
        .with_cookie_config(SessionCookieConfig::development())?;

    let mut application = Application::new();
    application.middleware(sessions.middleware())?;

    let csrf_route = csrf.clone();
    application.get("/csrf", move |request: Request| {
        let csrf = csrf_route.clone();
        async move {
            let token = csrf
                .token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf
                .cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;

            Ok(Response::json(&CsrfPayload {
                token: token.as_str().to_owned(),
            })?
            .with_header(http::header::SET_COOKIE, cookie)
            .with_header(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("no-store"),
            ))
        }
    })?;

    application.post("/login", move |request: Request| {
        let username = username.clone();
        let password_hash = password_hash.clone();
        async move {
            let form: LoginForm = request.json()?;
            let password_matches = password_hash
                .verify(&form.password)
                .map_err(|_| HttpError::internal_server_error())?;

            if form.username != username || !password_matches {
                return Err(HttpError::new(
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password",
                ));
            }

            // Only trusted server-side credential verification may assign identity and roles.
            let identity = AuthIdentity::new(username)
                .and_then(|identity| identity.with_role("admin"))
                .map_err(|_| HttpError::internal_server_error())?;
            request
                .session()
                .ok_or_else(HttpError::internal_server_error)?
                .authenticate(identity)
                .map_err(|_| HttpError::internal_server_error())?;

            Ok(Response::json(&LoginPayload {
                authenticated: true,
                subject: form.username,
            })?)
        }
    })?;
    application.route_middleware(HttpMethod::Post, "/login", csrf.clone().middleware())?;

    application.route_protected(HttpMethod::Get, "/me", |request: Request| async move {
        let identity = request
            .identity()
            .map_err(|_| HttpError::internal_server_error())?
            .ok_or_else(|| HttpError::new(StatusCode::UNAUTHORIZED, "Authentication required"))?;
        Response::json(&WhoAmIPayload {
            subject: identity.subject().to_owned(),
        })
    })?;

    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new().allow_role("admin.read", "admin")?,
    );
    application.route_authorized(
        HttpMethod::Get,
        "/admin",
        "admin.read",
        Some(policy),
        |_request: Request| async { Ok(Response::text("Authorized admin area")) },
    )?;

    application.route_protected(HttpMethod::Post, "/logout", |request: Request| async move {
        request
            .session()
            .ok_or_else(HttpError::internal_server_error)?
            .invalidate()
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Response::status(StatusCode::NO_CONTENT))
    })?;
    application.route_middleware(HttpMethod::Post, "/logout", csrf.middleware())?;

    application.get("/health", |_request: Request| async { Ok(Response::text("ok")) })?;

    Ok(application)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let environment = std::env::var("RUSTFLOW_ENV").unwrap_or_else(|_| "development".to_owned());
    if environment.eq_ignore_ascii_case("production") {
        return Err(
            "the authenticated example uses volatile sessions and a development cookie; it refuses production mode"
                .into(),
        );
    }

    let username = std::env::var("RUSTFLOW_DEMO_USER")?;
    let password = std::env::var("RUSTFLOW_DEMO_PASSWORD")?;
    let csrf_secret = std::env::var("RUSTFLOW_CSRF_KEY")?;
    if username.trim().is_empty() || password.is_empty() {
        return Err("RUSTFLOW_DEMO_USER and RUSTFLOW_DEMO_PASSWORD must be non-empty".into());
    }

    let password_hash = PasswordHash::hash(&password)?;
    let csrf = CsrfProtection::new(csrf_secret)?;
    let application = build_application(username, password_hash, csrf)?;

    let port = std::env::var("RUSTFLOW_AUTH_PORT")
        .unwrap_or_else(|_| "3001".to_owned())
        .parse::<u16>()?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("Could not install Ctrl+C handler: {error}");
        }
    };
    println!("RustFlow authenticated example listening on http://{address}");
    application.serve_with_shutdown(address, shutdown).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustflow_server::testing::TestApp;

    fn extract_cookie(response: &rustflow_server::testing::TestResponse, name: &str) -> String {
        let prefix = format!("{name}=");
        response
            .set_cookie_headers()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&prefix))
            .and_then(|value| value.split(';').next())
            .expect("expected response cookie")
            .to_owned()
    }

    fn test_app() -> TestApp {
        let password_hash =
            PasswordHash::hash("test-only-correct-password").expect("hash test password");
        let csrf = CsrfProtection::new("test-only-csrf-secret-with-at-least-32-bytes")
            .expect("valid test CSRF secret");
        let application = build_application("alice".to_owned(), password_hash, csrf)
            .expect("build authenticated application");
        TestApp::builder()
            .application(application)
            .build()
            .expect("assemble in-process test app")
    }

    async fn csrf_pair(app: &TestApp) -> (String, String) {
        let response = app
            .get("/csrf")
            .send()
            .await
            .expect("request a CSRF token");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: CsrfPayload = response.json().expect("decode CSRF payload");
        let cookie = extract_cookie(&response, "rustflow_csrf");
        (cookie, payload.token)
    }

    #[tokio::test]
    async fn authentication_authorization_csrf_and_logout_work_together() {
        let app = test_app();
        let anonymous = app.get("/me").send().await.expect("anonymous request");
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        let (csrf_cookie, csrf_token) = csrf_pair(&app).await;
        let csrf_cookie_value = csrf_cookie.split_once('=').expect("CSRF cookie pair").1;
        let origin = "http://localhost";

        let wrong_login = app
            .post("/login")
            .header("host", "localhost")
            .expect("host header")
            .header("origin", origin)
            .expect("origin header")
            .cookie("rustflow_csrf", csrf_cookie_value)
            .expect("CSRF cookie")
            .csrf_token(&csrf_token)
            .expect("CSRF header")
            .json(&LoginForm {
                username: "alice".to_owned(),
                password: "wrong-password".to_owned(),
            })
            .expect("serialize invalid credentials")
            .send()
            .await
            .expect("submit invalid credentials");
        assert_eq!(wrong_login.status(), StatusCode::UNAUTHORIZED);

        let login = app
            .post("/login")
            .header("host", "localhost")
            .expect("host header")
            .header("origin", origin)
            .expect("origin header")
            .cookie("rustflow_csrf", csrf_cookie_value)
            .expect("CSRF cookie")
            .csrf_token(&csrf_token)
            .expect("CSRF header")
            .json(&LoginForm {
                username: "alice".to_owned(),
                password: "test-only-correct-password".to_owned(),
            })
            .expect("serialize credentials")
            .send()
            .await
            .expect("submit valid credentials");
        assert_eq!(login.status(), StatusCode::OK);
        let login_payload: LoginPayload = login.json().expect("decode login result");
        assert!(login_payload.authenticated);
        assert_eq!(login_payload.subject, "alice");

        let session_cookie = extract_cookie(&login, "rustflow_session");
        let session_cookie_value = session_cookie.split_once('=').expect("session pair").1;
        let me = app
            .get("/me")
            .cookie("rustflow_session", session_cookie_value)
            .expect("session cookie")
            .send()
            .await
            .expect("read authenticated identity");
        assert_eq!(me.status(), StatusCode::OK);
        let me_payload: WhoAmIPayload = me.json().expect("decode identity response");
        assert_eq!(me_payload.subject, "alice");

        let admin = app
            .get("/admin")
            .cookie("rustflow_session", session_cookie_value)
            .expect("session cookie")
            .send()
            .await
            .expect("read protected admin route");
        assert_eq!(admin.status(), StatusCode::OK);
        assert_eq!(
            admin.text().expect("UTF-8 admin response"),
            "Authorized admin area"
        );

        let logout = app
            .post("/logout")
            .header("host", "localhost")
            .expect("host header")
            .header("origin", origin)
            .expect("origin header")
            .cookie("rustflow_csrf", csrf_cookie_value)
            .expect("CSRF cookie")
            .csrf_token(&csrf_token)
            .expect("CSRF header")
            .cookie("rustflow_session", session_cookie_value)
            .expect("session cookie")
            .send()
            .await
            .expect("logout");
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);

        let after_logout = app
            .get("/me")
            .cookie("rustflow_session", session_cookie_value)
            .expect("stale session cookie")
            .send()
            .await
            .expect("verify revoked session");
        assert_eq!(after_logout.status(), StatusCode::UNAUTHORIZED);
    }
}
