use tracing::{error, info};

const SERVICE_CONTENT: &str = "[Unit]\nDescription=Scety reverse proxy\nAfter=network.target\n\n[Service]\nExecStart={exe_path} run\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target";

pub async fn install() -> Result<(), Box<dyn std::error::Error>> {
    if !nix::unistd::Uid::effective().is_root() {
            error!("Run as root or with sudo");
            std::process::exit(1);
        }

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

        let service_content = SERVICE_CONTENT.replace("{exe_path}", &exe_path.to_string_lossy());

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