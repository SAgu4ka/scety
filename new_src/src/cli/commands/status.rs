use std::process::Command;
use std::thread;
use std::time::Duration;
use tracing::{info, warn};

pub fn get_status() -> Result<(String, u64), Box<dyn std::error::Error>> {
    let output = Command::new("systemctl")
        .args(["status", "scety"])
        .output()?;

    match output.status.code() {
        Some(0) => Ok(("active".to_owned(), 0)),
        Some(1) | Some(2) => Ok(("failed".to_owned(), 1)),
        Some(3) => Ok(("inactive".to_owned(), 2)),
        Some(4) => Ok(("missing".to_owned(), 3)),

        Some(other) => Err(format!("unknown code - {other}").into()),
        _ => Err("systemctl missing(likely killed by signal)".into()),
    }
}

pub fn status(
    follow: bool,
    checks: Option<u16>,
    interval: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    if follow {
        loop {
            let status = get_status()?;
            info!("Scety status is: {}", status.0);
            thread::sleep(Duration::from_secs(interval.unwrap_or(2).into()));
        }
    } else if let Some(count) = checks {
        let status = get_status()?;
        info!("Scety status is: {}", status.0);
        for _ in 0..count - 1 {
            thread::sleep(Duration::from_secs(interval.unwrap_or(2).into()));
            let status = get_status()?;
            info!("Scety status is: {}", status.0);
        }
        return Ok(());
    } else if interval.is_some() {
        warn!("The interval flag was passed but not used (try using it with other flags)");
    }
    let status = get_status()?;
    info!("Scety status is: {}", status.0);

    Ok(())
}
