use tracing::{debug, info, warn};

pub async fn stop() -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("systemctl")
        .args(["is-active", "scety"])
        .output()?;

    if status.stdout.trim_ascii() != b"active" {
        warn!("Scety is not running");
        return Err("Scety is not running".into());
    }

    debug!("Starting stopping scety...");
    std::process::Command::new("systemctl")
        .args(["stop", "scety"])
        .status()?;
    info!("Scety was stopped successfully");
    Ok(())
}