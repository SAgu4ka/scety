use tracing::{warn, info};

pub async fn reload() -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("systemctl")
        .args(["is-active", "scety"])
        .output()?;

    if status.stdout.trim_ascii() != b"active" {
        warn!("Scety is not running");
        return Err("Scety is not running yet! Use 'scety run' to start".into());
    }

    std::process::Command::new("systemctl")
        .args(["restart", "scety"])
        .status()?;

    info!("Scety restarted successfully");
    Ok(())
}