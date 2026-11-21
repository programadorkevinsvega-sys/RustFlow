use std::{env, net::SocketAddr, path::PathBuf};

use rustflow_server::{Application, ServerError, StaticFiles};

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let root = env::var_os("RUSTFLOW_STATIC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist"));
    let prefix = env::var("RUSTFLOW_STATIC_PREFIX").unwrap_or_else(|_| "/assets".to_owned());
    let port = env::var("RUSTFLOW_STATIC_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3000);

    let mut files = StaticFiles::new(root)?;
    if let Ok(index) = env::var("RUSTFLOW_STATIC_INDEX") {
        files = files.with_index_file(index)?;
    }

    let mut app = Application::new();
    app.static_files(prefix, files)?;
    app.serve(SocketAddr::from(([127, 0, 0, 1], port))).await
}
