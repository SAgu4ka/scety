use crate::config::get_services_config::ClientConfig;
use crate::http::error_pages::send;
use httparse::{EMPTY_HEADER, Request, Status};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tracing::{debug, error, info};

pub fn start_listen_port(
    port: u16,
    all_config_for_this_port: Vec<ClientConfig>,
    expose_version: bool,
) {
    info!(port=%port, "Start listen port");
    tokio::spawn(async move {
        let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(listener) => listener,
            Err(e) => {
                error!(error=%e, port=%port, "Error in starting listen port");
                return;
            }
        };

        loop {
            match tcp_listener.accept().await {
                Ok((socket, addr)) => {
                    debug!(client_ip=%addr, "New client connection");
                    let configs_clone = all_config_for_this_port.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(socket, configs_clone, expose_version).await {
                            error!(error=%e, port=%port, "Error handling client connection");
                        }
                    });
                }
                Err(e) => {
                    error!(error=%e, port=%port, "Error listening port")
                }
            }
        }
    });
}

async fn handle_client(
    mut client_socket: TcpStream,
    configs: Vec<ClientConfig>,
    expose_version: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("Starting client connection processing");

    let parse_result = timeout(Duration::from_secs(5), async {
        let mut buf = vec![0u8; 4096];
        let mut read_bytes = 0;

        loop {
            let n = match client_socket.read(&mut buf[read_bytes..]).await {
                Ok(0) => return Ok(None),
                Ok(n) => n,
                Err(e) => {
                    error!(error=%e, "Error reading socket");
                    return Err(e.into());
                }
            };

            read_bytes += n;

            let mut headers = [EMPTY_HEADER; 64];
            let mut req = Request::new(&mut headers);

            match req.parse(&buf[..read_bytes]) {
                Ok(Status::Complete(_)) => {
                    let host = headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case("Host"))
                        .map(|h| String::from_utf8_lossy(h.value).into_owned());

                    if let Some(req_host) = host {
                        let clean_host = req_host
                            .trim_start_matches("http://")
                            .trim_start_matches("https://")
                            .split(':')
                            .next()
                            .unwrap_or(&req_host)
                            .to_string();

                        if let Some(target_config) = configs
                            .iter()
                            .find(|cfg| cfg.host.as_deref() == Some(&clean_host))
                        {
                            return Ok(Some((
                                buf,
                                read_bytes,
                                target_config.upstream.as_ref().and_then(|u| u.port),
                            )));
                        }
                    }
                    return Ok(None);
                }
                Ok(Status::Partial) => {
                    if read_bytes >= buf.len() {
                        buf.resize(buf.len() * 2, 0);
                    }
                }
                Err(e) => {
                    error!(error=%e, "Error parsing HTTP request");
                    return Ok(None);
                }
            }
        }
    })
    .await;

    let client_ip = client_socket
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    match parse_result {
        Err(e) => {
            error!(client_ip=%client_ip, error=%e, "The client was reset due to a slow connection");
            send(&mut client_socket, 408, expose_version).await?;
            Ok(())
        }
        Ok(Err(e)) => {
            error!(client_ip=%client_ip, error=%e, "Error processing connection");
            Err(e)
        }
        Ok(Ok(None)) => {
            debug!(client_ip=%client_ip, "No configurations were found matching the client's request");
            send(&mut client_socket, 404, expose_version).await?;
            Ok(())
        }
        Ok(Ok(Some((buf, read_bytes, target_port)))) => {
            let target_addr = format!("127.0.0.1:{:?}", target_port);

            match timeout(Duration::from_secs(30), TcpStream::connect(&target_addr)).await {
                Err(e) => {
                    error!(error=%e, "Upstream connection timed out");
                    send(&mut client_socket, 504, expose_version).await?;
                    Ok(())
                }
                Ok(Err(e)) => {
                    error!(error=%e, "Upstream did not accept the connection");
                    send(&mut client_socket, 502, expose_version).await?;
                    Ok(())
                }
                Ok(Ok(mut upstream_socket)) => {
                    let header = format!("X-Forwarded-For: {}\r\n", client_ip);

                    let final_buf = if let Some(pos) =
                        buf[..read_bytes].windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        let mut new_buf = Vec::new();
                        new_buf.extend_from_slice(&buf[..pos + 2]);
                        new_buf.extend_from_slice(header.as_bytes());
                        new_buf.extend_from_slice(&buf[pos + 2..read_bytes]);
                        new_buf
                    } else {
                        buf[..read_bytes].to_vec()
                    };

                    debug!(target=%target_addr, "Proxying request to upstream");
                    upstream_socket.write_all(&final_buf).await?;
                    tokio::io::copy_bidirectional(&mut client_socket, &mut upstream_socket).await?;
                    Ok(())
                }
            }
        }
    }
}
