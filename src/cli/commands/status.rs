use tracing::warn;

pub async fn status() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new("systemctl")
        .args(["status", "scety"])
        .output()?;

    if output.stdout.is_empty() {
        warn!("Scety is not installed");
        return Ok(());
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}