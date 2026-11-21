use std::{error::Error, net::SocketAddr};

use rustflow_server::{
    ActionAccess, ActionConfig, ActionCsrf, Application, Request, ValidateInput, ValidationErrors,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PingInput {
    nonce: u64,
}

impl ValidateInput for PingInput {
    fn validate(&self) -> Result<(), ValidationErrors> {
        if self.nonce == 0 {
            let mut errors = ValidationErrors::new();
            errors.add_field_error("nonce", "The nonce must be greater than zero.");
            return Err(errors);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct PingOutput {
    nonce: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let action_config = ActionConfig::new(ActionAccess::Public, ActionCsrf::NotRequired)?;
    let mut application = Application::new();
    application.group("/api", move |api| {
        api.action(
            "/actions/ping",
            action_config,
            |_request: Request, input: PingInput| async move {
                Ok(PingOutput { nonce: input.nonce })
            },
        )?;
        Ok(())
    })?;

    let address = SocketAddr::from(([127, 0, 0, 1], 3001));
    println!("RustFlow server action example listening on http://{address}");
    application
        .serve_with_shutdown(address, async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("Could not install Ctrl+C handler: {error}");
            }
        })
        .await?;
    Ok(())
}
