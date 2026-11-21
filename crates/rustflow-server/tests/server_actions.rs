use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use http::StatusCode;
use rustflow_server::{
    AccessPolicy, ActionAccess, ActionConfig, ActionCsrf, Application, AuthIdentity,
    CsrfProtection, HttpError, Request, Response, RolePolicy, ServerConfig, SessionCookieConfig,
    SessionManager, ValidateInput, ValidationErrors, testing::TestApp,
};
use serde::{
    Deserialize, Serialize,
    ser::{SerializeSeq, Serializer},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionInput {
    value: String,
}

impl ValidateInput for ActionInput {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.value.trim().is_empty() {
            errors.add_field_error("value", "Enter a non-empty value.");
        }
        if self.value.len() > 64 {
            errors.add_field_error("value", "The value must be at most 64 bytes.");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ActionOutput {
    value: String,
}

struct PartialSerializationFailure;

impl Serialize for PartialSerializationFailure {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(2))?;
        sequence.serialize_element("partial-secret")?;
        Err(serde::ser::Error::custom("private serialization detail"))
    }
}

fn echo_app(config: ActionConfig) -> TestApp {
    let mut application = Application::new();
    application
        .action(
            "/actions/echo",
            config,
            |_request: Request, input: ActionInput| async move {
                Ok(ActionOutput { value: input.value })
            },
        )
        .expect("register action");
    TestApp::builder()
        .application(application)
        .build()
        .expect("build test app")
}

fn public_config() -> ActionConfig {
    ActionConfig::new(ActionAccess::Public, ActionCsrf::NotRequired)
        .expect("public action security")
}

#[test]
fn action_request_limit_cannot_exceed_the_global_body_limit() {
    let mut application = Application::with_config(ServerConfig::new().with_max_body_bytes(8));
    let error =
        match application.action(
            "/actions/too-large",
            public_config(),
            |_request: Request, input: ActionInput| async move {
                Ok(ActionOutput { value: input.value })
            },
        ) {
            Ok(_) => panic!("a per-action limit above the global limit must be rejected"),
            Err(error) => error,
        };
    assert!(matches!(
        error,
        rustflow_server::ServerError::ActionRequestLimitExceedsGlobal {
            action_limit: 65_536,
            global_limit: 8
        }
    ));

    let reduced = public_config()
        .with_max_request_bytes(8)
        .expect("valid reduced request limit");
    assert!(
        application
            .action(
                "/actions/reduced",
                reduced,
                |_request: Request, input: ActionInput| async move {
                    Ok(ActionOutput { value: input.value })
                },
            )
            .is_ok()
    );
}

#[tokio::test]
async fn json_action_validates_content_type_input_and_unknown_fields() {
    let app = echo_app(public_config());

    let success = app
        .post("/actions/echo")
        .json(&ActionInput {
            value: "accepted".to_owned(),
        })
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch action");
    assert_eq!(success.status(), StatusCode::OK);
    assert_eq!(
        success
            .header("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(
        success
            .header("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        success
            .json::<ActionOutput>()
            .expect("decode action result"),
        ActionOutput {
            value: "accepted".to_owned()
        }
    );

    let wrong_media_type = app
        .post("/actions/echo")
        .form_urlencoded(&ActionInput {
            value: "accepted".to_owned(),
        })
        .expect("serialize form")
        .send()
        .await
        .expect("dispatch wrong media type");
    assert_eq!(
        wrong_media_type.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );

    let duplicate_media_type = app
        .post("/actions/echo")
        .content_type("application/json")
        .expect("set JSON content type")
        .append_header("content-type", "text/plain")
        .expect("append conflicting content type")
        .body(r#"{"value":"accepted"}"#)
        .send()
        .await
        .expect("dispatch duplicate content type");
    assert_eq!(
        duplicate_media_type.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );

    let malformed = app
        .post("/actions/echo")
        .content_type("application/json")
        .expect("set content type")
        .body("{")
        .send()
        .await
        .expect("dispatch malformed JSON");
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

    let invalid = app
        .post("/actions/echo")
        .json(&ActionInput {
            value: "   ".to_owned(),
        })
        .expect("serialize invalid input")
        .send()
        .await
        .expect("dispatch invalid input");
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        invalid
            .text()
            .expect("validation response")
            .contains("Enter a non-empty value.")
    );
    assert!(!invalid.text().expect("validation response").contains("   "));

    let unknown_field = app
        .post("/actions/echo")
        .content_type("application/json")
        .expect("set content type")
        .body(r#"{"value":"ok","identity":"admin"}"#)
        .send()
        .await
        .expect("dispatch unknown field");
    assert_eq!(unknown_field.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn request_and_serialized_response_limits_fail_without_leaking_data() {
    let handler_called = Arc::new(AtomicBool::new(false));
    let called = Arc::clone(&handler_called);
    let config = public_config()
        .with_max_request_bytes(12)
        .expect("valid request limit");
    let mut application = Application::new();
    application
        .action(
            "/actions/limited",
            config,
            move |_request: Request, input: ActionInput| {
                let called = Arc::clone(&called);
                async move {
                    called.store(true, Ordering::SeqCst);
                    Ok(ActionOutput { value: input.value })
                }
            },
        )
        .expect("register request-limited action");

    let output_config = public_config()
        .with_max_response_bytes(4)
        .expect("valid response limit");
    application
        .action(
            "/actions/output-limited",
            output_config,
            |_request: Request, _input: ActionInput| async {
                Ok(ActionOutput {
                    value: "sensitive-output".to_owned(),
                })
            },
        )
        .expect("register response-limited action");

    let exact_output_config = public_config()
        .with_max_response_bytes(4)
        .expect("valid exact response limit");
    application
        .action(
            "/actions/output-exact",
            exact_output_config,
            |_request: Request, _input: ActionInput| async { Ok(true) },
        )
        .expect("register exact response action");

    application
        .action(
            "/actions/serialization-error",
            public_config(),
            |_request: Request, _input: ActionInput| async { Ok(PartialSerializationFailure) },
        )
        .expect("register failing serializer action");

    let app = TestApp::builder()
        .application(application)
        .build()
        .expect("build test app");
    let too_large_request = app
        .post("/actions/limited")
        .json(&ActionInput {
            value: "a request too large for this action".to_owned(),
        })
        .expect("serialize oversized request")
        .send()
        .await
        .expect("dispatch oversized request");
    assert_eq!(too_large_request.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!handler_called.load(Ordering::SeqCst));

    let too_large_response = app
        .post("/actions/output-limited")
        .json(&ActionInput {
            value: "input".to_owned(),
        })
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch output-limited action");
    assert_eq!(
        too_large_response.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(
        !too_large_response
            .text()
            .expect("generic server error")
            .contains("sensitive-output")
    );

    let exact_output = app
        .post("/actions/output-exact")
        .json(&ActionInput {
            value: "input".to_owned(),
        })
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch exact-limit action");
    assert_eq!(exact_output.status(), StatusCode::OK);
    assert_eq!(exact_output.body(), b"true");

    let serialization_error = app
        .post("/actions/serialization-error")
        .json(&ActionInput {
            value: "input".to_owned(),
        })
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch serialization-error action");
    assert_eq!(
        serialization_error.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let error_body = serialization_error.text().expect("generic server error");
    assert!(!error_body.contains("partial-secret"));
    assert!(!error_body.contains("private serialization detail"));
}

#[tokio::test]
async fn cookie_protected_actions_enforce_csrf_authentication_and_permissions() {
    let csrf = CsrfProtection::new([0x41_u8; 32]).expect("CSRF key");
    let sessions = SessionManager::in_memory(Duration::from_secs(300))
        .expect("session manager")
        .with_cookie_config(SessionCookieConfig::development())
        .expect("development cookie");
    let protected_called = Arc::new(AtomicBool::new(false));
    let protected_flag = Arc::clone(&protected_called);
    let admin_called = Arc::new(AtomicBool::new(false));
    let admin_flag = Arc::clone(&admin_called);

    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new()
            .allow_role("admin.write", "admin")
            .expect("role policy"),
    );
    let mut application = Application::new();
    application
        .middleware(sessions.middleware())
        .expect("install sessions");
    application
        .post("/test-login", |request: Request| async move {
            let session = request
                .session()
                .ok_or_else(HttpError::internal_server_error)?;
            session
                .create()
                .map_err(|_| HttpError::internal_server_error())?;
            let identity = AuthIdentity::new("alice")
                .and_then(|identity| identity.with_role("member"))
                .map_err(|_| HttpError::internal_server_error())?;
            session
                .authenticate(identity)
                .map_err(|_| HttpError::internal_server_error())?;
            Ok(Response::status(StatusCode::NO_CONTENT))
        })
        .expect("register test login");
    application
        .action(
            "/actions/public-csrf",
            ActionConfig::new(ActionAccess::Public, ActionCsrf::Required(csrf.clone()))
                .expect("public CSRF security"),
            |_request: Request, input: ActionInput| async move {
                Ok(ActionOutput { value: input.value })
            },
        )
        .expect("register CSRF action");
    let csrf_limited_config =
        ActionConfig::new(ActionAccess::Public, ActionCsrf::Required(csrf.clone()))
            .expect("public CSRF security")
            .with_max_request_bytes(4)
            .expect("CSRF action limit");
    application
        .action(
            "/actions/public-csrf-limited",
            csrf_limited_config,
            |_request: Request, input: ActionInput| async move {
                Ok(ActionOutput { value: input.value })
            },
        )
        .expect("register CSRF-limited action");
    application
        .action(
            "/actions/private",
            ActionConfig::new(
                ActionAccess::Authenticated,
                ActionCsrf::Required(csrf.clone()),
            )
            .expect("authenticated action security"),
            move |_request: Request, input: ActionInput| {
                let called = Arc::clone(&protected_flag);
                async move {
                    called.store(true, Ordering::SeqCst);
                    Ok(ActionOutput { value: input.value })
                }
            },
        )
        .expect("register authenticated action");
    application
        .action(
            "/actions/admin",
            ActionConfig::new(
                ActionAccess::permission("admin.write", policy).expect("valid permission"),
                ActionCsrf::Required(csrf.clone()),
            )
            .expect("authorized action security"),
            move |_request: Request, input: ActionInput| {
                let called = Arc::clone(&admin_flag);
                async move {
                    called.store(true, Ordering::SeqCst);
                    Ok(ActionOutput { value: input.value })
                }
            },
        )
        .expect("register authorized action");

    let app = TestApp::builder()
        .application(application)
        .build()
        .expect("build test app");
    let input = ActionInput {
        value: "ok".to_owned(),
    };
    let anonymous = app
        .post("/actions/private")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch anonymous action");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert!(!protected_called.load(Ordering::SeqCst));

    let no_csrf = app
        .post("/actions/public-csrf")
        .header("origin", "http://localhost")
        .expect("set origin")
        .header("host", "localhost")
        .expect("set host")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch without CSRF");
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);

    let oversized_before_csrf = app
        .post("/actions/public-csrf-limited")
        .header("origin", "http://localhost")
        .expect("set origin")
        .header("host", "localhost")
        .expect("set host")
        .content_type("application/x-www-form-urlencoded")
        .expect("set form media type")
        .body("value=too-large")
        .send()
        .await
        .expect("dispatch oversized CSRF request");
    assert_eq!(
        oversized_before_csrf.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let token = csrf.issue_token().expect("issue token");
    let cross_origin = app
        .post("/actions/public-csrf")
        .header("origin", "http://attacker.example")
        .expect("set cross-origin origin")
        .header("host", "localhost")
        .expect("set host")
        .cookie("rustflow_csrf", token.as_str())
        .expect("set CSRF cookie")
        .csrf_token(token.as_str())
        .expect("set CSRF header")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch cross-origin request");
    assert_eq!(cross_origin.status(), StatusCode::FORBIDDEN);

    let csrf_ok = app
        .post("/actions/public-csrf")
        .header("origin", "http://localhost")
        .expect("set origin")
        .header("host", "localhost")
        .expect("set host")
        .cookie("rustflow_csrf", token.as_str())
        .expect("set CSRF cookie")
        .csrf_token(token.as_str())
        .expect("set CSRF header")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch with valid CSRF");
    assert_eq!(csrf_ok.status(), StatusCode::OK);

    let login = app
        .post("/test-login")
        .send()
        .await
        .expect("establish member session");
    assert_eq!(login.status(), StatusCode::NO_CONTENT);
    let session_cookie = login
        .set_cookie_headers()
        .filter_map(|header| header.to_str().ok())
        .find(|header| header.starts_with("rustflow_session="))
        .and_then(|header| header.split(';').next())
        .expect("session cookie");
    let (cookie_name, cookie_value) = session_cookie.split_once('=').expect("cookie pair");

    let authenticated = app
        .post("/actions/private")
        .cookie(cookie_name, cookie_value)
        .expect("set session cookie")
        .cookie("rustflow_csrf", token.as_str())
        .expect("set CSRF cookie")
        .csrf_token(token.as_str())
        .expect("set CSRF token")
        .header("origin", "http://localhost")
        .expect("set origin")
        .header("host", "localhost")
        .expect("set host")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch authenticated action");
    assert_eq!(authenticated.status(), StatusCode::OK);

    let denied = app
        .post("/actions/admin")
        .cookie(cookie_name, cookie_value)
        .expect("set session cookie")
        .cookie("rustflow_csrf", token.as_str())
        .expect("set CSRF cookie")
        .csrf_token(token.as_str())
        .expect("set CSRF token")
        .header("origin", "http://localhost")
        .expect("set origin")
        .header("host", "localhost")
        .expect("set host")
        .json(&input)
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch denied action");
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(!admin_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn action_errors_are_generic_and_timeouts_cancel_the_handler() {
    let dropped = Arc::new(AtomicBool::new(false));
    struct DropSignal(Arc<AtomicBool>);
    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let mut application = Application::with_config(
        ServerConfig::new().with_request_timeout(Duration::from_millis(20)),
    );
    let drop_signal = Arc::clone(&dropped);
    application
        .action(
            "/actions/slow",
            public_config(),
            move |_request: Request, _input: ActionInput| {
                let drop_signal = Arc::clone(&drop_signal);
                async move {
                    let _signal = DropSignal(drop_signal);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok(ActionOutput {
                        value: "late".to_owned(),
                    })
                }
            },
        )
        .expect("register slow action");

    let app = TestApp::builder()
        .application(application)
        .build()
        .expect("build test app");
    let response = app
        .post("/actions/slow")
        .json(&ActionInput {
            value: "request".to_owned(),
        })
        .expect("serialize request")
        .send()
        .await
        .expect("dispatch slow action");
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    assert!(dropped.load(Ordering::SeqCst));
}
