use crate::config::settings::{EXPOSE_VERSION, SERVICES_CONFIGS_PATH};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, warn};

const NO_CONFIG_HTML: &str = include_str!("../models/no_configs.html");
const ENGINE: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");

pub async fn start_fallback_server() -> Result<(), Box<dyn std::error::Error>> {
    let server_header = if EXPOSE_VERSION {
        ENGINE.to_string()
    } else {
        ENGINE_NAME.to_string()
    };

    let bind_target = "0.0.0.0:80";
    let listener = TcpListener::bind(&bind_target).await?;
    warn!(address=%bind_target, config_path=%SERVICES_CONFIGS_PATH, "Configs not found, starting fallback server");

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
                        Ok(0) => (),
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
}
