use std::{error::Error, net::SocketAddr};

use rustflow::view::View;
use rustflow_server::{
    Application, HttpError, HttpMethod, Middleware, Request, Response, html_document,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
