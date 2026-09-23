use crate::cli::commands::status::get_status;
use std::process::Command;
use tracing::{error, info};

pub fn stop(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (_, status_id) = get_status()?;

    match status_id {
        0..=2 => {
            if force {
                info!("Force stopping scety...");
                Command::new("systemctl").args(["kill", "scety"]).status()?;
                info!("Scety force stopped successfully");
                return Ok(());
            }

            info!("Stopping scety...");
            Command::new("systemctl").args(["stop", "scety"]).status()?;
            info!("Scety stopped successfully");
            Ok(())
        }
        3 => {
            error!("Scety is not installed yet!");
            Err("Scety is not installed yet!".into())
        }
        other => {
            error!("Unexpected service status ID: {}", other);
            Err(format!("Unexpected service status ID: {other}").into())
        }
    }
}
