use crate::{
    DynResult, PID_PATH,
    cli::commands::{run::verify_systemd_run, status::get_status},
};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::fs;
use std::process::Command;
use tracing::{error, info, warn};

pub async fn reload() -> DynResult<()> {
    let is_systemd = match verify_systemd_run() {
        Ok(_) => true,
        Err(e) => {
            warn!(error = %e, "systemd check skipped");
            false
        }
    };

    if is_systemd {
        let pid_str = fs::read_to_string(PID_PATH)
            .map_err(|_| "Main process not running (PID file not found)")?;

        let pid_num: i32 = pid_str.trim().parse()?;
        let pid = Pid::from_raw(pid_num);

        kill(pid, Signal::SIGUSR1)?;
        info!(
            pid = pid_num,
            "The reload signal was successfully sent to the main process"
        );
        Ok(())
    } else {
        let (_, status_id) = get_status()?;

        match status_id {
            0..=2 => {
                info!("Restarting scety...");
                Command::new("systemctl")
                    .args(["restart", "scety"])
                    .status()?;
                info!("Scety restarted successfully");
                return Ok(());
            }
            3 => {
                error!("Scety is not installed yet!");
                return Err("Scety is not installed yet!".into());
            }
            other => {
                error!(id = other, "Unexpected service status ID");
                return Err(format!("Unexpected service status ID: {other}").into());
            }
        }
    }
}
