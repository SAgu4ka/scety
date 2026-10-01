use crate::{MAIN_SCETY_PATH, SCETY_USER, cli::commands::status::get_status};
use nix::unistd::User;
use std::{
    fs::{remove_dir_all, remove_file},
    io::{Error, ErrorKind},
    path::Path,
    process::Command,
};
use tracing::{debug, error, info, warn};

pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    if !nix::unistd::Uid::effective().is_root() {
        error!("Run as root or with sudo");
        return Err(Box::new(Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Run as root or with sudo",
        )));
    }

    info!("Start uninstalling scety... (T_T)");

    let scety_config_path = Path::new(MAIN_SCETY_PATH).join("scety.toml");
    let service_path = Path::new("/etc/systemd/system/scety.service");
    let exe_path = std::env::current_exe()?;

    if !scety_config_path.exists() {
        warn!("Scety is not installed");
        return Err(Box::new(Error::new(
            ErrorKind::NotFound,
            "Scety is not installed",
        )));
    }

    if let (_, 0 | 1) = get_status()? {
        info!("Stopping Scety before uninstall");
        if !Command::new("systemctl")
            .args(["stop", "scety"])
            .status()?
            .success()
        {
            warn!("Failed to stop scety service via systemctl");
        }
    }

    debug!("Disabling scety service via systemd");
    if !Command::new("systemctl")
        .args(["disable", "scety"])
        .status()?
        .success()
    {
        warn!("Failed to disable scety service via systemctl");
    }

    if service_path.exists() {
        debug!(path = ?service_path, "Deleting scety-service file");
        remove_file(service_path)?;
    } else {
        warn!(file = ?service_path, "Service file does not exist");
    }

    debug!("Reloading systemd daemon");
    if !Command::new("systemctl")
        .args(["daemon-reload"])
        .status()?
        .success()
    {
        warn!("Failed to reload systemd daemon");
    }

    if Path::new(MAIN_SCETY_PATH).exists() {
        debug!(path = MAIN_SCETY_PATH, "Removing scety directory");
        remove_dir_all(MAIN_SCETY_PATH)?;
    } else {
        warn!(path = MAIN_SCETY_PATH, "Scety directory does not exist");
    }

    if User::from_name(SCETY_USER)?.is_some() {
        debug!(user = SCETY_USER, "Deleting the scety user");
        if !Command::new("userdel").arg(SCETY_USER).status()?.success() {
            warn!(user = SCETY_USER, "Failed to delete user");
        }
    } else {
        warn!(user = SCETY_USER, "User not exists");
    }

    if exe_path.exists() {
        debug!(path = ?exe_path, "Deleting exe");
        if let Err(err) = remove_file(&exe_path)
            && err.kind() != ErrorKind::NotFound
        {
            warn!(path = ?exe_path, error = %err, "Failed to delete exe file");
        }
    }

    info!("Scety successfully uninstalled... (T_T)");
    Ok(())
}
