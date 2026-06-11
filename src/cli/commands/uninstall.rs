use tracing::{info, warn};

pub async fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    if !std::path::Path::new("/etc/systemd/system/scety.service").exists() {
        warn!("Scety is not installed");
        return Err("Scety is not installed".into());
    }

    let status = std::process::Command::new("systemctl")
        .args(["is-active", "scety"])
        .output()?;

    if status.stdout.trim_ascii() == b"active" {
        std::process::Command::new("systemctl")
            .args(["stop", "scety"])
            .status()?;
    }

    std::process::Command::new("systemctl")
        .args(["disable", "scety"])
        .status()?;

    std::fs::remove_file("/etc/systemd/system/scety.service")?;

    std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status()?;

    info!("Scety successfully uninstalled");
    Ok(())
}