use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::{CONFIG_PATH, EXPOSE_VERSION};
use crate::network::global_router::start_listen;

mod http;
mod core;
mod config;
mod network;

const NO_CONFIG_HTML: &str = include_str!("./models/no_configs.html");
const ENGINE: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    let all_configs = get_all_configs();
    if all_configs.len() == 0 {
        let server_header = if EXPOSE_VERSION {
            ENGINE.to_string()
        } else {
            ENGINE_NAME.to_string()
        };

        let bind_target = "0.0.0.0:80";
        let listener = TcpListener::bind(&bind_target).await?;
        println!("[scety] Configs not found, start server on http://{} , config path: {}", bind_target, CONFIG_PATH);

        loop {
            let (mut socket, _) = listener.accept().await?;

            let server_header_clone = server_header.clone();
            tokio::spawn(async move {
                let mut buf = [0; 1024];

                let html_body = NO_CONFIG_HTML.replace("{{ENGINE}}", &server_header_clone);
                        
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

                match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => return,
                    Ok(_) => {
                        if let Err(e) = socket.write_all(response.as_bytes()).await {
                            eprintln!("Error writing to socket: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Error reading socket: {}", e),
                }
            });
        }
    } else {
        start_listen(all_configs, EXPOSE_VERSION);
    }

    Ok(()) 
}