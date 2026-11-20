use std::{error::Error, net::SocketAddr, rc::Rc, sync::Arc};

use rustflow::components::{
    ComponentRender, SharedComponentCatalogBuilder, SharedComponentDefinition,
};
use rustflow::view::{Property, PropertyValue, View};
use rustflow_server::{
    Application, HtmlPage, HttpError, HttpMethod, Middleware, Request, Response, ValidateInput,
    ValidationErrors, html_document,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Deserialize)]
struct UserQuery {
    detailed: Option<bool>,
}

#[derive(Serialize)]
struct UserResponse {
    id: String,
    detailed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct EchoPayload {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignupPayload {
    email: String,
    age: u8,
}

impl ValidateInput for SignupPayload {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if !self.email.contains('@') {
            errors.add_field_error("email", "Enter a valid email address.");
        }
        if self.age < 18 {
            errors.add_field_error("age", "You must be at least 18 years old.");
        }
        if !errors.is_empty() {
            errors.add_form_error("Please correct the highlighted fields.");
            return Err(errors);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct SignupResponse {
    accepted: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Build the immutable component catalog once during application startup.
    let article = SharedComponentDefinition::new("ArticlePage", |_context, _initial| {
        let render: ComponentRender = Rc::new(|properties, _children| {
            let id = properties
                .iter()
                .find(|property| property.name() == "id")
                .and_then(|property| match property.value() {
                    PropertyValue::Text(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "unknown".to_owned());
            Ok(View::element(
                "article",
                [View::element("h1", [View::text(id)])?],
            )?)
        });
        Ok(render)
    })?
    .server_only();
    let layout = SharedComponentDefinition::new("AppLayout", |_context, _properties| {
        let render: ComponentRender =
            Rc::new(|_properties, children| Ok(View::element("main", children.to_vec())?));
        Ok(render)
    })?;
    let mut catalog_builder = SharedComponentCatalogBuilder::new();
    catalog_builder.register(article)?;
    catalog_builder.register(layout)?;
    let catalog = Arc::new(catalog_builder.build());

    let mut app = Application::new();

    app.middleware(Middleware::new(|request, next| async move {
        let response = next.run(request).await?;
        Ok(response.with_header(
            http::HeaderName::from_static("x-rustflow-example"),
            http::HeaderValue::from_static("middleware"),
        ))
    }))?;

    app.get("/", |_request: Request| async {
        let heading = View::element("h1", [View::text("RustFlow server")])
            .map_err(|_| HttpError::internal_server_error())?;
        let note = View::element("p", [View::text("HTML estructural, rutas tipadas y JSON.")])
            .map_err(|_| HttpError::internal_server_error())?;
        let page = View::element("main", [heading, note])
            .map_err(|_| HttpError::internal_server_error())?;
        html_document("RustFlow server", "es", &page)
    })?;

    // This handler captures the shared catalog; definitions are not rebuilt per request.
    let article_catalog = Arc::clone(&catalog);
    app.get("/articles/{id}", move |request: Request| {
        let catalog = Arc::clone(&article_catalog);
        async move {
            let id = request.path_param("id").unwrap_or("unknown").to_owned();
            let property =
                Property::new("id", id).map_err(|_| HttpError::internal_server_error())?;
            HtmlPage::from_component("Article", "ArticlePage", [property], [])?
                .language("es")
                .with_shared_components(&catalog)
                .with_layout("AppLayout", [])?
                .render()
        }
    })?;

    app.group("/api", |api| {
        api.name_prefix("api")?;
        api.route_named(
            "health",
            HttpMethod::Get,
            "/health",
            |_request: Request| async { Response::json(&HealthResponse { status: "ok" }) },
        )?;
        api.post("/echo", |request: Request| async move {
            let payload = request.json::<EchoPayload>()?;
            Response::json(&payload)
        })?;
        api.post("/signup", |request: Request| async move {
            let _signup = request.validated_json::<SignupPayload>()?;
            Response::json(&SignupResponse { accepted: true })
        })?;
        Ok(())
    })?;

    app.get("/users/{id}", |request: Request| async move {
        let query = request.query::<UserQuery>()?;
        let response = UserResponse {
            id: request.path_param("id").unwrap_or("unknown").to_owned(),
            detailed: query.detailed.unwrap_or(false),
        };
        Response::json(&response)
    })?;

    for route in app.routes() {
        println!("{:?} {} ({:?})", route.method, route.path, route.name);
    }

    let address = SocketAddr::from(([127, 0, 0, 1], 3000));
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("Could not install Ctrl+C handler: {error}");
        }
    };
    app.serve_with_shutdown(address, shutdown).await?;
    Ok(())
}
