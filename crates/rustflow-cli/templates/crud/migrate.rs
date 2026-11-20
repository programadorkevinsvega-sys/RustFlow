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
    if let Err(error) = database.run_migrations(directory).await {
        eprintln!("migration directory: {directory}");
        let mut current: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        while let Some(cause) = current {
            eprintln!("migration error: {cause}");
            current = cause.source();
        }
        return Err(error.into());
    }
    database.close().await;
    println!("RustFlow CRUD migrations completed.");
    Ok(())
}
