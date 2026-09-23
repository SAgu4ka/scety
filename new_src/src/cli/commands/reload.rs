use crate::cli::commands::status::get_status;
use std::process::Command;
use tracing::{error, info};

pub fn reload() -> Result<(), Box<dyn std::error::Error>> {
    let (_, status_id) = get_status()?;

    match status_id {
        0..=2 => {
            info!("Restarting scety...");
            Command::new("systemctl")
                .args(["restart", "scety"])
                .status()?;
            info!("Scety restarted successfully");
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
