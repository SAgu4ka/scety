use std::time::Duration;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use crate::config::get_services_config::ClientConfig;
use crate::http::error_pages::send;
use httparse::{EMPTY_HEADER, Request, Status};

pub fn start_listen_port(port: u16, all_config_for_this_port: Vec<ClientConfig>, expose_version: bool) {
    tokio::spawn( async move {
        let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(listener) => listener,
            Err(_) => {
                return;
            }
        };

        loop {
            match tcp_listener.accept().await {
                Ok((socket, _)) => {
                    let configs_clone = all_config_for_this_port.clone();

                    tokio::spawn(async move {
                        if let Err(_) = handle_client(socket, configs_clone, expose_version).await {

                        }
                    });
                }
                Err(_) => {}
            }
        }
    });
}

async fn handle_client(
    mut client_socket: TcpStream,
    configs: Vec<ClientConfig>,
    expose_version: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = vec![0u8; 4096];
    let mut read_bytes = 0;

    loop {
        let timeout_result = timeout(
            Duration::from_secs(5),
            client_socket.read(&mut buf[read_bytes..])
        ).await;

        let n = match  timeout_result {
            Ok(Ok(0)) => return Ok(()),
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                return Ok(());
            }
        };

        read_bytes += n;

        let mut headers = [EMPTY_HEADER; 64];
        let mut req = Request::new(&mut headers);

        match req.parse(&buf[..read_bytes]) {
            Ok(Status::Complete(_header_len)) => {
                let host = headers.iter()
                    .find(|header| header.name.eq_ignore_ascii_case("Host"))
                    .map(|header| String::from_utf8_lossy(header.value).into_owned());
                
                if let Some(req_host) = host {
                    let clean_host = req_host
                        .trim_start_matches("http://")
                        .trim_start_matches("https://")
                        .split(":")
                        .next()
                        .unwrap_or(&req_host);

                    if let Some(target_config) = configs.iter().find(|cfg| cfg.host == clean_host) {
                        let target_addr = format!("127.0.0.1:{}", target_config.target_port);
                        let mut upstream_socket = TcpStream::connect(&target_addr).await?;

                        upstream_socket.write_all(&buf[..read_bytes]).await?;
                        tokio::io::copy_bidirectional(&mut client_socket, &mut upstream_socket).await?;
                        return Ok(());
                    }
                }
                break;
            }
            Ok(httparse::Status::Partial) => {
                if read_bytes >= buf.len() {
                    buf.resize(buf.len() * 2, 0);
                }
            }
            Err(_) => return Ok(()),
        }
    }
    send(&mut client_socket, 404, expose_version).await?;

    Ok(())
}
