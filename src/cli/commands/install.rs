use crate::config::settings::{MAIN_SCETY_PATH, SCETY_CONFIG_PATH};
use nix::unistd::{Group, User, chown};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tracing::{error, info, warn};

const SCETY_USER: &str = "scety";
const ACME_CACHE_PATH: &str = "{main_path}/acme-cache";

const SERVICE_CONTENT: &str = "\
[Unit]
Description=Scety reverse proxy
After=network.target

[Service]
ExecStart={exe_path} run
Restart=on-failure
RestartSec=5

User=scety
Group=scety
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true

ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths={main_path}/acme-cache
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
# SystemCallFilter=@system-service

[Install]
WantedBy=multi-user.target";
const SCETY_CONFIG: &str = include_str!("../../models/default_scety_config.toml");

pub async fn install() -> Result<(), Box<dyn std::error::Error>> {
    if !nix::unistd::Uid::effective().is_root() {
        error!("Run as root or with sudo");
        std::process::exit(1);
    }

    ensure_system_user()?;
    maybe_join_ssl_cert_group()?;

    let config_path = Path::new(SCETY_CONFIG_PATH);

    if !config_path.exists() {
        if let Some(parent_dir) = config_path.parent() {
            fs::create_dir_all(parent_dir)?;
            info!(dir = %parent_dir.display(), "Configuration directory created");
        }
        fs::write(config_path, SCETY_CONFIG)?;
        info!(path = %SCETY_CONFIG_PATH, "Default configuration file created");
    }

    configure_permissions(config_path)?;

    if std::path::Path::new("/etc/systemd/system/scety.service").exists() {
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

    let service_content = SERVICE_CONTENT.replace(
        "{exe_path}",
        &exe_path
            .to_string_lossy()
            .replace("{main_path}", MAIN_SCETY_PATH),
    );

    std::fs::write("/etc/systemd/system/scety.service", service_content)?;
    info!("Service file created");
    std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status()?;

    std::process::Command::new("systemctl")
        .args(["enable", "--now", "scety"])
        .status()?;

    info!("Scety successfully installed and started as a systemd service");
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

    let acme_cache_path = ACME_CACHE_PATH.replace("{main_path}", MAIN_SCETY_PATH);

    fs::create_dir_all(&acme_cache_path)?;
    chown(Path::new(&acme_cache_path), Some(uid), Some(gid))?;
    fs::set_permissions(&acme_cache_path, fs::Permissions::from_mode(0o700))?;

    Ok(())
}
