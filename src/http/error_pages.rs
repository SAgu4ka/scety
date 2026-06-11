use crate::http::generate_http_response::{generate_text_response, get_status_message};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{debug, info};

const ERROR_TEMPLATE: &str = include_str!("../models/error_template.html");
const ENGINE: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");

fn generate_html(code: u16, message: &str, expose_version: bool) -> String {
    debug!(error_code=%code, "Generating an HTML page for an error");

    let engine_display = if expose_version { ENGINE } else { ENGINE_NAME };

    ERROR_TEMPLATE
        .replace("{{CODE}}", &code.to_string())
        .replace("{{MESSAGE}}", message)
        .replace("{{ENGINE}}", engine_display)
}

pub async fn send(stream: &mut TcpStream, code: u16, expose_version: bool) -> std::io::Result<()> {
    let client_ip = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    info!(client_ip=%client_ip, "Sending an error to the client");
    let message = get_status_message(code);
    let html_content = generate_html(code, message, expose_version);

    let response = generate_text_response(
        code,
        "close",
        true,
        Some("text/html; charset=utf-8"),
        Some(&html_content),
        expose_version,
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}
