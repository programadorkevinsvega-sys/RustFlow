use std::error::Error;

use rustflow_server::{AppConfig, DatabaseBackend, DatabaseConfig, DatabasePool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load()?;
    let database_config = DatabaseConfig::from_app_config(&config)?;
    let database = DatabasePool::connect(&database_config).await?;
    let directory = match database.backend() {
        DatabaseBackend::Sqlite => "migrations/sqlite",
        DatabaseBackend::Postgres => "migrations/postgres",
        DatabaseBackend::MySql => "migrations/mysql",
    };
    database.run_migrations(directory).await?;
    database.close().await;
    println!("RustFlow CRUD migrations completed.");
    Ok(())
}
