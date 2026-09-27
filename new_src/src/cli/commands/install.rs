use nix::unistd::{Group, User, chown};
use std::os::unix::fs::PermissionsExt;
use std::{fs, io::Error, path::Path};
use tracing::{error, info, warn};

use crate::{SCETY_USER, settings::MAIN_SCETY_PATH};

const MAIN_SCETY_CONFIG: &str = include_str!("../../models/default_scety_config.toml");

const SERVICE_CONTENT: &str = "\
[Unit]
Description=Scety reverse proxy
After=network.target

[Service]
Type=exec
ExecStart={exe_path} run
ExecReload={exe_path} reload
Restart=on-failure
RestartSec=5

User={scety_user}
Group={scety_user}

AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true

ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

StateDirectory=scety

ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
ProtectClock=true
ProtectHostname=true
RestrictSUIDSGID=true
RestrictRealtime=true
RestrictNamespaces=true
LockPersonality=true
MemoryDenyWriteExecute=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
SystemCallArchitectures=native

SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM

[Install]
WantedBy=multi-user.target";

pub fn install(force_reinstall: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !nix::unistd::Uid::effective().is_root() {
        error!("Run as root or with sudo");
        return Err(Box::new(Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Run as root or with sudo",
        )));
    }

    ensure_system_user()?;
    maybe_join_ssl_cert_group()?;

    let scety_config_path = Path::new(MAIN_SCETY_PATH).join("scety.toml");

    if force_reinstall || !scety_config_path.exists() {
        if let Some(parent_dir) = scety_config_path.parent() {
            fs::create_dir_all(parent_dir)?;
            info!(dir = %parent_dir.display(), "Configuration directory created");
        }
        fs::write(&scety_config_path, MAIN_SCETY_CONFIG)?;
        info!(path = %scety_config_path.display(), "Configuration file created/updated");
    }

    configure_permissions(&scety_config_path)?;

    let service_path = Path::new("/etc/systemd/system/scety.service");

    if !force_reinstall && service_path.exists() {
        let status = std::process::Command::new("systemctl")
            .args(["is-active", "scety"])
            .output()?;

        if status.stdout.trim_ascii() == b"active" {
            info!("Scety is already running");
            return Ok(());
        }

        info!("Scety is already installed, starting...");
        std::process::Command::new("systemctl")
            .args(["start", "scety"])
            .status()?;
        return Ok(());
    }

    let exe_path = std::env::current_exe()?;
    let exe_str = exe_path.to_str().ok_or("Invalid executable path string")?;

    let service_content = SERVICE_CONTENT
        .replace("{exe_path}", exe_str)
        .replace("{scety_user}", SCETY_USER);

    fs::write(service_path, service_content)?;
    info!("Service file created/updated");

    std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status()?;

    if force_reinstall {
        std::process::Command::new("systemctl")
            .args(["restart", "scety"])
            .status()?;
        info!("Scety successfully reinstalled and restarted as a systemd service");
    } else {
        std::process::Command::new("systemctl")
            .args(["enable", "--now", "scety"])
            .status()?;
        info!("Scety successfully installed and started as a systemd service");
    }

    Ok(())
}
fn ensure_system_user() -> Result<(), Box<dyn std::error::Error>> {
    if User::from_name(SCETY_USER)?.is_some() {
        return Ok(());
    }

    info!(user = %SCETY_USER, "Creating dedicated system user for scety");
    let status = std::process::Command::new("useradd")
        .args([
            "--system",
            "--no-create-home",
            "--shell",
            "/usr/sbin/nologin",
            "--user-group",
            SCETY_USER,
        ])
        .status()?;

    if !status.success() {
        return Err(format!("useradd exited with status {status}").into());
    }

    Ok(())
}

fn maybe_join_ssl_cert_group() -> Result<(), Box<dyn std::error::Error>> {
    if Group::from_name("ssl-cert")?.is_none() {
        return Ok(());
    }

    info!(
        "Detected 'ssl-cert' group (typical for certbot-managed certificates) — adding scety to it"
    );
    let status = std::process::Command::new("usermod")
        .args(["-aG", "ssl-cert", SCETY_USER])
        .status()?;

    if !status.success() {
        warn!(
            "Could not add scety to 'ssl-cert' group automatically; if you use certbot-managed \
             certificates, add it manually: usermod -aG ssl-cert scety"
        );
    }

    Ok(())
}

fn configure_permissions(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let scety = User::from_name(SCETY_USER)?.ok_or("scety user must exist by this point")?;
    let uid = scety.uid;
    let gid = scety.gid;

    if let Some(config_dir) = config_path.parent() {
        for entry in walkdir::WalkDir::new(config_dir) {
            let entry = entry?;
            chown(entry.path(), None, Some(gid))?;
            let mode = if entry.file_type().is_dir() {
                0o750
            } else {
                0o640
            };
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))?;
        }
    }

    let acme_cache_path = Path::new(MAIN_SCETY_PATH).join("acme-cache");

    fs::create_dir_all(&acme_cache_path)?;
    chown(Path::new(&acme_cache_path), Some(uid), Some(gid))?;
    fs::set_permissions(&acme_cache_path, fs::Permissions::from_mode(0o700))?;

    Ok(())
}
