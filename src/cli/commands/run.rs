use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::{SERVISES_CONFIGS_PATH, EXPOSE_VERSION};
use crate::network::global_router::start_listen;
use tracing::{warn, error, info, debug};
use std::sync::Arc;

const NO_CONFIG_HTML: &str = include_str!("../../models/no_configs.html");
const ENGINE: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let is_systemd = std::env::var("INVOCATION_ID").is_ok();

    if is_systemd {
        info!("Start scety...");
        debug!(config_path=%SERVISES_CONFIGS_PATH, expose_version=%EXPOSE_VERSION, "Starting arguments");
        debug!("Start load configs...");
        let all_configs = get_all_configs();
        if all_configs.is_empty() {
            let server_header = if EXPOSE_VERSION {
                ENGINE.to_string()
            } else {
                ENGINE_NAME.to_string()
            };

            let bind_target = "0.0.0.0:80";
            let listener = TcpListener::bind(&bind_target).await?;
            warn!(address=%bind_target, config_path=%SERVISES_CONFIGS_PATH, "Configs not found, starting fallback server");

            let html_body = NO_CONFIG_HTML.replace("{{ENGINE}}", &server_header);
            
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                Content-Type: text/html; charset=utf-8\r\n\
                Content-Length: {}\r\n\
                Connection: close\r\n\
                \r\n\
                {}", 
                html_body.len(), 
                html_body
            );
            let response = Arc::new(response);
            
            loop {
                match listener.accept().await {
                    Ok((mut socket, _)) => {
                        let response = Arc::clone(&response);
                        tokio::spawn(async move {
                            let mut buf = [0; 1024];

                            match socket.read(&mut buf).await {
                                Ok(n) if n == 0 => return,
                                Ok(_) => {
                                    if let Err(e) = socket.write_all(response.as_bytes()).await {
                                        error!(error=%e, "Error writing to socket");
                                    }
                                }
                                Err(e) => error!(error=%e, "Error reading socket"),
                            }
                        });
                    }
                    Err(e) => {
                        error!(error=%e, "Failed to accept incoming connection on fallback server");
                    }
                } 
                
            }
        } else {
            info!("Successfully loaded {} configs", all_configs.len());
            info!("Start listeners...");
            start_listen(all_configs, EXPOSE_VERSION);
            tokio::signal::ctrl_c().await?;
        }
        Ok(())
    } else {
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

        let service_content = format!(r#"[Unit]
Description=Scety reverse proxy
After=network.target

[Service]
ExecStart={} run
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
"#, exe_path.display());

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
}