use crate::config::get_scety_config::scety_config;
use crate::config::get_services_config::{ClientConfig, SslConfig};
use crate::core::search_router::SearchRouter;
use crate::http::error_pages::send;
use crate::network::ip_limit;
use crate::network::tls::{build_acme_config, load_manual_tls};
use futures::StreamExt;
use httparse::{EMPTY_HEADER, Request, Status};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

pub enum SslMode {
    None,
    Manual(SslConfig),
    Acme(SslConfig),
}

pub async fn start_listen_port(
    port: u16,
    all_config_for_this_port: Vec<ClientConfig>,
    expose_version: bool,
    ssl_mode: SslMode,
    token: CancellationToken,
) {
    let search_router = Arc::new(SearchRouter::new(all_config_for_this_port));

    match ssl_mode {
        SslMode::None => start_plain_port(port, search_router, expose_version, token).await,
        SslMode::Manual(ssl) => {
            start_manual_tls_port(port, &ssl, search_router, expose_version, token).await
        }
        SslMode::Acme(ssl) => {
            start_acme_port(port, &ssl, search_router, expose_version, token).await
        }
    }
}

fn check_ip_limit(ip: IpAddr) -> Result<ip_limit::ConnectionGuard, ()> {
    let limit = scety_config().ip_limitation.unwrap_or(20);
    ip_limit::try_acquire(ip, limit)
}

async fn start_plain_port(
    port: u16,
    search_router: Arc<SearchRouter>,
    expose_version: bool,
    token: CancellationToken,
) {
    info!(port=%port, "Start listen port (plain)");
    let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            error!(error=%e, port=%port, "Error in starting listen port");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                info!(port=%port, "Listener shutting down");
                break;
            }

            result = tcp_listener.accept() => {
                match result {
                    Ok((socket, addr)) => {
                        debug!(client_ip=%addr, "New client connection");

                        let limit = scety_config().ip_limitation.unwrap_or(20);
                        let guard = match check_ip_limit(addr.ip()) {
                            Ok(guard) => guard,
                            Err(()) => {
                                debug!(client_ip=%addr, limit=%limit, "Connection limit reached for this IP");
                                let mut socket = socket;
                                tokio::spawn(async move {
                                    send(&mut socket, 429, expose_version).await.ok();
                                });
                                continue;
                            }
                        };

                        let child = token.child_token();
                        let search_router = Arc::clone(&search_router);
                        let client_ip = addr.to_string();

                        tokio::spawn(async move {
                            let _guard = guard;
                            handle_client(socket, client_ip, expose_version, child, search_router).await
                        });
                    }
                    Err(e) => error!(error=%e, port=%port, "Error listening port"),
                }
            }
        }
    }
}

async fn start_manual_tls_port(
    port: u16,
    ssl: &SslConfig,
    search_router: Arc<SearchRouter>,
    expose_version: bool,
    token: CancellationToken,
) {
    info!(port=%port, "Start listen port (manual TLS)");

    let acceptor = match load_manual_tls(ssl) {
        Ok(a) => a,
        Err(e) => {
            error!(error=%e, port=%port, "Failed to load TLS certificate/key");
            return;
        }
    };

    let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            error!(error=%e, port=%port, "Error in starting listen port");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                info!(port=%port, "TLS listener shutting down");
                break;
            }

            result = tcp_listener.accept() => {
                match result {
                    Ok((socket, addr)) => {
                        debug!(client_ip=%addr, "New TLS client connection");

                        let guard = match check_ip_limit(addr.ip()) {
                            Ok(guard) => guard,
                            Err(()) => {
                                debug!(client_ip=%addr, "Connection limit reached for this IP");
                                continue;
                            }
                        };

                        let acceptor = acceptor.clone();
                        let child = token.child_token();
                        let search_router = Arc::clone(&search_router);
                        let client_ip = addr.to_string();

                        tokio::spawn(async move {
                            let _guard = guard;
                            match acceptor.accept(socket).await {
                                Ok(tls_stream) => {
                                    let _ = handle_client(tls_stream, client_ip, expose_version, child, search_router).await;
                                }
                                Err(e) => {
                                    error!(error=%e, client_ip=%client_ip, "TLS handshake failed");
                                }
                            }
                        });
                    }
                    Err(e) => error!(error=%e, port=%port, "Error listening port"),
                }
            }
        }
    }
}

async fn start_acme_port(
    port: u16,
    ssl: &SslConfig,
    search_router: Arc<SearchRouter>,
    expose_version: bool,
    token: CancellationToken,
) {
    info!(port=%port, "Start listen port (ACME TLS)");

    let tcp_listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => {
            error!(error=%e, port=%port, "Failed to bind");
            return;
        }
    };

    let tcp_incoming = TcpListenerStream::new(tcp_listener);
    let mut tls_incoming =
        build_acme_config(ssl).incoming(tcp_incoming, vec![b"http/1.1".to_vec()]);

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                info!(port=%port, "ACME listener shutting down");
                break;
            }
            item = tls_incoming.next() => {
                match item {
                    None => break,
                    Some(Err(e)) => {
                        error!(error=%e, "ACME/TLS error");
                    }
                    Some(Ok(tls_stream)) => {
                        let client_ip = tls_stream
                            .get_ref().0
                            .peer_addr()
                            .map(|addr| addr.to_string())
                            .unwrap_or_else(|_| "Unknown".to_string());

                        let router = Arc::clone(&search_router);
                        let child = token.child_token();
                        tokio::spawn(async move {
                            let _ = handle_client(tls_stream, client_ip, expose_version, child, router).await;
                        });
                    }
                }
            }
        }
    }
}

async fn handle_client<S>(
    mut client_socket: S,
    client_ip: String,
    expose_version: bool,
    token: CancellationToken,
    search_router: Arc<SearchRouter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    debug!("Starting client connection processing");

    tokio::select! {
        _ = token.cancelled() => {
            send(&mut client_socket, 503, expose_version).await.ok();
            Ok(())
        }
        result = process_request(&mut client_socket, &client_ip, expose_version, search_router) => {
            result
        }
    }
}

async fn process_request<S>(
    client_socket: &mut S,
    client_ip: &str,
    expose_version: bool,
    search_router: Arc<SearchRouter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let cfg = scety_config();

    if cfg.client_use_full_timeout {
        return match cfg.client_full_timeout {
            Some(d) => {
                match timeout(
                    d,
                    process_request_inner(client_socket, client_ip, expose_version, search_router),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        debug!(client_ip=%client_ip, "Connection closed: full timeout exceeded");
                        Ok(())
                    }
                }
            }
            None => {
                process_request_inner(client_socket, client_ip, expose_version, search_router).await
            }
        };
    }

    process_request_inner(client_socket, client_ip, expose_version, search_router).await
}

async fn process_request_inner<S>(
    client_socket: &mut S,
    client_ip: &str,
    expose_version: bool,
    search_router: Arc<SearchRouter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let cfg = scety_config();
    let max_buf = cfg.client_header_buffer.unwrap();

    let parse_result = match cfg.client_headers_timeout {
        Some(d) => {
            timeout(
                d,
                read_request(
                    client_socket,
                    max_buf as usize,
                    client_ip,
                    expose_version,
                    search_router,
                ),
            )
            .await
        }
        None => Ok(read_request(
            client_socket,
            max_buf as usize,
            client_ip,
            expose_version,
            search_router,
        )
        .await),
    };

    match parse_result {
        Err(e) => {
            error!(client_ip=%client_ip, error=%e, "The client was reset due to a slow connection");
            send(&mut *client_socket, 408, expose_version).await?;
            Ok(())
        }
        Ok(Err(e)) => {
            error!(client_ip=%client_ip, error=%e, "Error processing connection");
            Err(e)
        }
        Ok(Ok(None)) => {
            debug!(client_ip=%client_ip, "No configurations were found matching the client's request");
            send(&mut *client_socket, 404, expose_version).await?;
            Ok(())
        }
        Ok(Ok(Some((buf, read_bytes, target_port)))) => {
            let target_addr = match target_port {
                Some(port) => format!("127.0.0.1:{}", port),
                None => {
                    send(&mut *client_socket, 502, expose_version).await?;
                    return Ok(());
                }
            };

            match timeout(Duration::from_secs(30), TcpStream::connect(&target_addr)).await {
                Err(e) => {
                    error!(error=%e, "Upstream connection timed out");
                    send(&mut *client_socket, 504, expose_version).await?;
                    Ok(())
                }
                Ok(Err(e)) => {
                    error!(error=%e, "Upstream did not accept the connection");
                    send(&mut *client_socket, 502, expose_version).await?;
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

                    let copy_fut =
                        tokio::io::copy_bidirectional(&mut *client_socket, &mut upstream_socket);

                    match scety_config().client_body_timeout {
                        Some(d) => match timeout(d, copy_fut).await {
                            Ok(result) => {
                                result?;
                                Ok(())
                            }
                            Err(_) => {
                                debug!(client_ip=%client_ip, "Connection closed: body timeout exceeded");
                                Ok(())
                            }
                        },
                        None => {
                            copy_fut.await?;
                            Ok(())
                        }
                    }
                }
            }
        }
    }
}

async fn read_request<S>(
    client_socket: &mut S,
    max_buf: usize,
    client_ip: &str,
    expose_version: bool,
    search_router: Arc<SearchRouter>,
) -> Result<Option<(Vec<u8>, usize, Option<u16>)>, Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let mut read_bytes = 0;
    let mut buf = vec![0u8; max_buf.min(4096)];
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

                    if let Some(target_config) = search_router.find(&clean_host) {
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
                    if buf.len() >= max_buf {
                        error!(client_ip=%client_ip, "The buffer is full");
                        send(&mut *client_socket, 431, expose_version).await?;
                        return Ok(None);
                    }
                    let new_len = (buf.len() * 2).min(max_buf);
                    buf.resize(new_len, 0);
                }
            }
            Err(e) => {
                error!(error=%e, "Error parsing HTTP request");
                return Ok(None);
            }
        }
    }
}
