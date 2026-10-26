use std::{error::Error, net::SocketAddr};

use rustflow::view::View;
use rustflow_server::{Application, HttpError, Request, Response, html_view};
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

    app.get("/", |_request: Request| async {
        let heading = View::element("h1", [View::text("RustFlow server")])
            .map_err(|_| HttpError::internal_server_error())?;
        let note = View::element("p", [View::text("HTML estructural, rutas tipadas y JSON.")])
            .map_err(|_| HttpError::internal_server_error())?;
        let page = View::element("main", [heading, note])
            .map_err(|_| HttpError::internal_server_error())?;
        html_view(&page)
    })?;

    app.get("/api/health", |_request: Request| async {
        Response::json(&HealthResponse { status: "ok" })
    })?;

    app.get("/users/{id}", |request: Request| async move {
        let query = request.query::<UserQuery>()?;
        let response = UserResponse {
            id: request.path_param("id").unwrap_or("unknown").to_owned(),
            detailed: query.detailed.unwrap_or(false),
        };
        Response::json(&response)
    })?;

    app.post("/api/echo", |request: Request| async move {
        let payload = request.json::<EchoPayload>()?;
        Response::json(&payload)
    })?;

    app.serve(SocketAddr::from(([127, 0, 0, 1], 3000))).await?;
    Ok(())
}
