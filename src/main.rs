use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::http::error_pages::send;

mod http;
mod core;

const NO_CONFIG_HTML: &str = include_str!("./models/no_configs.html");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expose_version = true;
    let bind_addr = "0.0.0.0";
    let bind_port = 8080;
    let bind_target = format!("{}:{}", bind_addr, bind_port);
    let listener = TcpListener::bind(&bind_target).await?;
    println!("Scety start in http://{}", bind_target);

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = [0; 1024];

            match socket.read(&mut buf).await {
                Ok(n) if n == 0 => return,
                Ok(n) => {
                    let request = String::from_utf8_lossy(&buf[..n]);
                    println!("Получен запрос:\n{}", request);

                    let first_line = request.lines().next().unwrap_or("");
                    
                    let mut parts = first_line.split_whitespace();
                    let _method = parts.next().unwrap_or("");
                    let path = parts.next().unwrap_or("");
                    
                    let code_str = path.strip_prefix("/").unwrap_or("");
                    
                    if let Ok(code) = code_str.parse::<u16>() {
                        if let Err(e) = send(&mut socket, code, expose_version).await {
                            eprintln!("Error sending HTML error response: {}", e);
                        }
                    } else if path == "/" {
                        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}", NO_CONFIG_HTML.len(), NO_CONFIG_HTML);
                        if let Err(e) = socket.write_all(response.as_bytes()).await {
                            eprintln!("Error in writing in socket: {}", e);
                        }
                    } else {
                        if let Err(e) = send(&mut socket, 404, expose_version).await {
                            eprintln!("Error sending 404 for invalid path: {}", e);
                        }
                    }
                }
                Err(e) => eprintln!("Error read socket: {}", e),
            }
        });
    }
}