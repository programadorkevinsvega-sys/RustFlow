use std::{error::Error, net::SocketAddr, sync::Arc, time::Duration};

use rustflow_server::{
    Application, HttpMethod, InMemoryRateLimitStore, RateLimit, Request, Response,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut application = Application::new();
    application.get("/limited", |_request: Request| async {
        Ok(Response::text("accepted"))
    })?;

    let store = Arc::new(InMemoryRateLimitStore::new());
    let policy = RateLimit::new(store, "example.limited", 30, Duration::from_secs(1))?;
    application.route_middleware(HttpMethod::Get, "/limited", policy.middleware())?;

    let port = std::env::var("RUSTFLOW_RATE_LIMIT_PORT")
        .unwrap_or_else(|_| "3002".to_owned())
        .parse::<u16>()?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("Could not install Ctrl+C handler: {error}");
        }
    };
    println!("RustFlow rate-limit example listening on http://{address}");
    application.serve_with_shutdown(address, shutdown).await?;
    Ok(())
}
