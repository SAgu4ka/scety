use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use crate::config::get_servises_config::ClientConfig;
use crate::http::{parse_http_request::parse_http_request, error_pages::send};

pub fn start_listen_port(port: u16, all_config_for_this_port: Vec<ClientConfig>, expose_version: bool) {
    tokio::spawn( async move {
        let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(listener) => listener,
            Err(e) => {
                return;
            }
        };

        loop {
            match tcp_listener.accept().await {
                Ok((socket, _)) => {
                    let configs_clone = all_config_for_this_port.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(socket, configs_clone, expose_version).await {

                        }
                    });
                }
                Err(e) => {}
            }
        }
    });
}

async fn handle_client(
    mut client_socket: TcpStream,
    configs: Vec<ClientConfig>,
    expose_version: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 4096];

    let n = client_socket.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request_str = String::from_utf8_lossy(&buf[..n]);
    let _request = parse_http_request(&request_str);

    if let Some(target_config) = configs.first() {
        let target_addr = format!("127.0.0.1:{}", target_config.target_port);
        let mut upstream_socket = TcpStream::connect(&target_addr).await?;

        upstream_socket.write_all(&buf[..n]).await?;

        tokio::io::copy_bidirectional(&mut client_socket, &mut upstream_socket).await?;
    } else {
        send(&mut client_socket, 404, expose_version);
    }

    Ok(())
}
