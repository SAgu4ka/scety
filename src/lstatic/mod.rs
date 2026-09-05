#![allow(dead_code)]
pub mod config;
mod http3;

pub struct StaticModule;
impl crate::core::runtime::ProxyModule for StaticModule {
    const PROXY_TYPE: &'static str = "STATIC";
}

pub fn has_configs(raw_configs: &[toml::Value]) -> bool {
    raw_configs.iter().any(is_static_config)
}

use crate::core::host_router::HostRouter;
use config::{Files, StaticConfig, TransportProtocol};
use futures::StreamExt;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub async fn start(raw_configs: Vec<toml::Value>, token: CancellationToken) {
    let raw_configs = raw_configs
        .into_iter()
        .filter(is_static_config)
        .flat_map(|value| {
            value
                .get("configs")
                .and_then(toml::Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![value])
        })
        .collect();
    let configs = config::parse_static_configs(raw_configs);
    if configs.is_empty() {
        warn!("No valid static configurations remain after validation");
        token.cancelled().await;
        return;
    }
    let mut by_port: HashMap<u16, Vec<StaticConfig>> = HashMap::new();
    for config in configs {
        by_port.entry(config.port).or_default().push(config);
    }

    let mut listeners = JoinSet::new();
    for (port, configs) in by_port {
        let listener_token = token.child_token();
        listeners.spawn(async move { start_port(port, configs, listener_token).await });
    }
    while listeners.join_next().await.is_some() {}
}

fn is_static_config(value: &toml::Value) -> bool {
    value
        .get("proxy_type")
        .and_then(toml::Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("static"))
}

async fn start_port(port: u16, configs: Vec<StaticConfig>, token: CancellationToken) {
    if configs.len() == 1 {
        match &configs[0].mode {
            TransportProtocol::RawTcp { .. } => {
                let Some(config) = configs.into_iter().next() else {
                    return;
                };
                start_raw_tcp_port(port, config, token).await;
                return;
            }
            TransportProtocol::RawUdp { .. } => {
                let Some(config) = configs.into_iter().next() else {
                    return;
                };
                start_raw_udp_port(port, config, token).await;
                return;
            }
            TransportProtocol::Http3 { .. } => {
                let Some(config) = configs.into_iter().next() else {
                    return;
                };
                http3::start(port, config, token).await;
                return;
            }
            TransportProtocol::Tcp { .. } => {}
        }
    }
    let tls_ssl = match configs.first().map(|config| &config.mode) {
        Some(TransportProtocol::Tcp { ssl: Some(ssl), .. }) => Some(ssl.clone()),
        _ => None,
    };
    if configs.len() == 1
        && let Some(ssl) = tls_ssl
    {
        let Some(config) = configs.into_iter().next() else {
            return;
        };
        start_tls_http_port(port, config, ssl, token).await;
        return;
    }
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, port, "Static listener bind failed");
            return;
        }
    };
    start_tcp_listener(listener, configs, token).await;
}

async fn start_tcp_listener(
    listener: TcpListener,
    configs: Vec<StaticConfig>,
    token: CancellationToken,
) {
    let mut router = HostRouter::new();
    for (index, config) in configs.iter().enumerate() {
        router.add_pattern(&config.host, index);
    }
    let router = Arc::new(router);
    let configs = Arc::new(configs);
    info!(address = ?listener.local_addr(), "Static listener started");

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _)) => {
                    let router = Arc::clone(&router);
                    let configs = Arc::clone(&configs);
                    tokio::spawn(async move { serve_connection(stream, router, configs).await; });
                }
                Err(error) => error!(%error, "Static accept failed"),
            }
        }
    }
}

async fn start_raw_tcp_port(port: u16, config: StaticConfig, token: CancellationToken) {
    let (path, ssl) = match config.mode {
        TransportProtocol::RawTcp { ssl, path } => (path, ssl),
        _ => return,
    };
    if let Some(ssl) = ssl {
        start_tls_raw_tcp_port(port, path, ssl, token).await;
        return;
    }
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, port, "Raw TCP static listener bind failed");
            return;
        }
    };
    let content = match tokio::fs::read(&path).await {
        Ok(content) => Arc::new(content),
        Err(error) => {
            error!(%error, path = %path.display(), "Raw TCP static file read failed");
            return;
        }
    };
    info!(port, path = %path.display(), "Raw TCP static listener started");
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = listener.accept() => match result {
                Ok((mut stream, _)) => {
                    let content = Arc::clone(&content);
                    tokio::spawn(async move { let _ = stream.write_all(&content).await; });
                }
                Err(error) => error!(%error, port, "Raw TCP static accept failed"),
            }
        }
    }
}

async fn start_tls_raw_tcp_port(
    port: u16,
    path: std::path::PathBuf,
    ssl: crate::_core::SslConfig,
    token: CancellationToken,
) {
    let Some(acceptor) = load_tls_acceptor(&ssl) else {
        if ssl.acme.unwrap_or(false) {
            error!(port, "ACME is not supported for raw TCP static mode");
        }
        return;
    };
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, port, "TLS raw TCP static listener bind failed");
            return;
        }
    };
    let content = match tokio::fs::read(&path).await {
        Ok(content) => Arc::new(content),
        Err(error) => {
            error!(%error, path = %path.display(), "TLS raw TCP static file read failed");
            return;
        }
    };
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _)) => {
                    let acceptor = acceptor.clone();
                    let content = Arc::clone(&content);
                    tokio::spawn(async move {
                        if let Ok(mut stream) = acceptor.accept(stream).await {
                            let _ = stream.write_all(&content).await;
                        }
                    });
                }
                Err(error) => error!(%error, port, "TLS raw TCP static accept failed"),
            }
        }
    }
}

async fn start_acme_http_port(
    port: u16,
    config: StaticConfig,
    ssl: crate::_core::SslConfig,
    token: CancellationToken,
) {
    let domains = ssl.acme_domains.clone().unwrap_or_default();
    let email = ssl.acme_email.clone().unwrap_or_default();
    let cache = ssl
        .acme_cache
        .clone()
        .unwrap_or_else(|| format!("{}/acme-cache", crate::config::settings::MAIN_SCETY_PATH));
    let acme = tokio_rustls_acme::AcmeConfig::new(domains)
        .contact_push(format!("mailto:{email}"))
        .cache(tokio_rustls_acme::caches::DirCache::new(cache));
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, port, "ACME static listener bind failed");
            return;
        }
    };
    let mut incoming = acme.incoming(
        tokio_stream::wrappers::TcpListenerStream::new(listener),
        vec![b"http/1.1".to_vec()],
    );
    let mut router = HostRouter::new();
    router.add_pattern(&config.host, 0);
    let router = Arc::new(router);
    let configs = Arc::new(vec![config]);
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = incoming.next() => match result {
                Some(Ok(stream)) => {
                    let router = Arc::clone(&router);
                    let configs = Arc::clone(&configs);
                    tokio::spawn(async move { serve_connection(stream, router, configs).await; });
                }
                Some(Err(error)) => warn!(%error, port, "ACME static handshake failed"),
                None => break,
            }
        }
    }
}

async fn start_tls_http_port(
    port: u16,
    config: StaticConfig,
    ssl: crate::_core::SslConfig,
    token: CancellationToken,
) {
    if ssl.acme.unwrap_or(false) {
        start_acme_http_port(port, config, ssl, token).await;
        return;
    }
    let Some(acceptor) = load_tls_acceptor(&ssl) else {
        return;
    };
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, port, "TLS static listener bind failed");
            return;
        }
    };
    let mut router = HostRouter::new();
    router.add_pattern(&config.host, 0);
    let router = Arc::new(router);
    let configs = Arc::new(vec![config]);
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _)) => {
                    let acceptor = acceptor.clone();
                    let router = Arc::clone(&router);
                    let configs = Arc::clone(&configs);
                    tokio::spawn(async move {
                        if let Ok(stream) = acceptor.accept(stream).await {
                            serve_connection(stream, router, configs).await;
                        }
                    });
                }
                Err(error) => error!(%error, port, "TLS static accept failed"),
            }
        }
    }
}

fn load_tls_acceptor(ssl: &crate::_core::SslConfig) -> Option<TlsAcceptor> {
    if ssl.acme.unwrap_or(false) {
        return None;
    }
    let cert_path = ssl.cert.as_deref()?;
    let key_path = ssl.key.as_deref()?;
    let certs = CertificateDer::pem_file_iter(cert_path)
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let key = PrivateKeyDer::from_pem_file(key_path).ok()?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .ok()?;
    Some(TlsAcceptor::from(Arc::new(config)))
}

async fn start_raw_udp_port(port: u16, config: StaticConfig, token: CancellationToken) {
    let path = match config.mode {
        TransportProtocol::RawUdp { path } => path,
        _ => return,
    };
    let socket = match tokio::net::UdpSocket::bind(("0.0.0.0", port)).await {
        Ok(socket) => socket,
        Err(error) => {
            error!(%error, port, "Raw UDP static listener bind failed");
            return;
        }
    };
    let content = match tokio::fs::read(&path).await {
        Ok(content) => content,
        Err(error) => {
            error!(%error, path = %path.display(), "Raw UDP static file read failed");
            return;
        }
    };
    start_raw_udp_socket(socket, content, token, path).await;
}

async fn start_raw_udp_socket(
    socket: tokio::net::UdpSocket,
    content: Vec<u8>,
    token: CancellationToken,
    path: std::path::PathBuf,
) {
    let mut request = [0u8; 65536];
    info!(address = ?socket.local_addr(), path = %path.display(), "Raw UDP static listener started");
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = socket.recv_from(&mut request) => match result {
                Ok((_, peer)) => {
                    if let Err(error) = socket.send_to(&content, peer).await {
                        warn!(%error, %peer, "Raw UDP static response failed");
                    }
                }
                Err(error) => error!(%error, address = ?socket.local_addr(), "Raw UDP static receive failed"),
            }
        }
    }
}

async fn serve_connection<S>(
    mut stream: S,
    router: Arc<HostRouter>,
    configs: Arc<Vec<StaticConfig>>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut request = [0u8; 16 * 1024];
    let read = match stream.read(&mut request).await {
        Ok(read) => read,
        Err(error) => {
            warn!(%error, "Static request read failed");
            return;
        }
    };
    let Some((method, path, host)) = parse_request(&request[..read]) else {
        write_response(&mut stream, 400, "text/plain", b"Bad Request", true).await;
        return;
    };
    if method != "GET" && method != "HEAD" {
        write_response(&mut stream, 405, "text/plain", b"Method Not Allowed", true).await;
        return;
    }
    let Some(config) = router.matches(host).and_then(|index| configs.get(index)) else {
        write_response(&mut stream, 404, "text/plain", b"Not Found", true).await;
        return;
    };
    let Some(file) = resolve_file(&config.mode, path) else {
        write_response(&mut stream, 404, "text/plain", b"Not Found", true).await;
        return;
    };
    let (root, relative) = file;
    if relative.split('/').any(|part| part == "..") {
        write_response(&mut stream, 400, "text/plain", b"Bad Request", true).await;
        return;
    }
    let root = match std::fs::canonicalize(root) {
        Ok(root) => root,
        Err(_) => {
            write_response(&mut stream, 404, "text/plain", b"Not Found", true).await;
            return;
        }
    };
    let file = root.join(if relative.is_empty() {
        "index.html"
    } else {
        relative
    });
    let file = match tokio::fs::canonicalize(&file).await {
        Ok(file) if file.starts_with(&root) => file,
        _ => {
            write_response(&mut stream, 404, "text/plain", b"Not Found", true).await;
            return;
        }
    };
    let content = match tokio::fs::read(&file).await {
        Ok(content) => content,
        Err(_) => {
            write_response(&mut stream, 404, "text/plain", b"Not Found", true).await;
            return;
        }
    };
    write_response(
        &mut stream,
        200,
        content_type(&file),
        &content,
        method == "GET",
    )
    .await;
}

fn resolve_file<'a>(
    mode: &'a TransportProtocol,
    request_path: &'a str,
) -> Option<(&'a Path, &'a str)> {
    match mode {
        TransportProtocol::Tcp { files, .. } => match files {
            Files::SimpleRootDir(path) => {
                let relative = request_path.trim_start_matches('/');
                Some((
                    path,
                    if relative.is_empty() {
                        "index.html"
                    } else {
                        relative
                    },
                ))
            }
            Files::RootDir(config) => {
                let relative = request_path.trim_start_matches('/');
                Some((
                    &config.path,
                    if relative.is_empty() {
                        config.index_file.as_str()
                    } else {
                        relative
                    },
                ))
            }
            Files::Map(map) => map.iter().find_map(|(prefix, path)| {
                let relative = request_path.strip_prefix(prefix)?;
                if prefix != "/" && !relative.is_empty() && !relative.starts_with('/') {
                    return None;
                }
                Some((path.as_path(), relative.trim_start_matches('/')))
            }),
        },
        _ => None,
    }
}

fn parse_request(request: &[u8]) -> Option<(&str, &str, &str)> {
    let text = std::str::from_utf8(request).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?;
    let target = first.next()?.split('?').next()?;
    let host = lines
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
                .map(|(_, value)| value)
        })?
        .trim();
    Some((method, target, host.split(':').next().unwrap_or(host)))
}

async fn write_response<S>(
    stream: &mut S,
    code: u16,
    content_type: &str,
    body: &[u8],
    include_body: bool,
) where
    S: AsyncWrite + Unpin,
{
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        501 => "Not Implemented",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    if include_body {
        let _ = stream.write_all(body).await;
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    #[test]
    fn detects_static_module_documents() {
        let value: toml::Value = toml::from_str(
            r#"
            proxy_type = "STATIC"
            [[configs]]
            name = "site"
            host = "example.com"
            port = 8080
            [configs.mode]
            protocol = "tcp"
            http_version = "auto"
            files = "/tmp"
            "#,
        )
        .unwrap();

        assert!(has_configs(&[value]));
    }

    #[test]
    fn parses_host_case_insensitively_and_removes_query() {
        assert_eq!(
            parse_request(b"GET /index.html?x=1 HTTP/1.1\r\nhOsT: example.com\r\n\r\n"),
            Some(("GET", "/index.html", "example.com"))
        );
    }

    #[test]
    fn rejects_requests_without_host() {
        assert_eq!(parse_request(b"GET / HTTP/1.1\r\n\r\n"), None);
    }

    #[test]
    fn files_map_requires_path_boundary() {
        let files = Files::Map(std::collections::HashMap::from([(
            "/assets".to_string(),
            std::path::PathBuf::from("/srv/assets"),
        )]));

        assert!(
            resolve_file(
                &TransportProtocol::Tcp {
                    ssl: None,
                    http_version: config::HttpVersion::Auto,
                    files,
                },
                "/assets-old/app.js"
            )
            .is_none()
        );
    }

    #[tokio::test]
    async fn serves_static_file_through_real_handler() {
        let root = std::env::temp_dir().join(format!(
            "scety_static_e2e_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("index.html"), b"hello from static").unwrap();

        let config = StaticConfig {
            name: "e2e".to_string(),
            host: "example.com".to_string(),
            port: 0,
            mode: TransportProtocol::Tcp {
                ssl: None,
                http_version: config::HttpVersion::Auto,
                files: Files::SimpleRootDir(root.clone()),
            },
        };
        let mut host_router = HostRouter::new();
        host_router.add_pattern("example.com", 0);
        let (mut client, server_stream) = tokio::io::duplex(4096);
        let server = tokio::spawn(serve_connection(
            server_stream,
            Arc::new(host_router),
            Arc::new(vec![config]),
        ));

        client
            .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        let _ = std::fs::remove_dir_all(root);

        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with(b"hello from static"));
    }

    #[tokio::test]
    async fn serves_static_file_through_real_listener() {
        let root = std::env::temp_dir().join(format!(
            "scety_static_listener_e2e_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("index.html"), b"listener response").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let token = CancellationToken::new();
        let config = StaticConfig {
            name: "listener-e2e".to_string(),
            host: "example.com".to_string(),
            port: address.port(),
            mode: TransportProtocol::Tcp {
                ssl: None,
                http_version: config::HttpVersion::Auto,
                files: Files::SimpleRootDir(root.clone()),
            },
        };
        let listener_token = token.clone();
        let server = tokio::spawn(start_tcp_listener(listener, vec![config], listener_token));
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        token.cancel();
        server.await.unwrap();
        let _ = std::fs::remove_dir_all(root);

        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with(b"listener response"));
    }

    #[tokio::test]
    async fn serves_raw_udp_file_through_real_listener() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let token = CancellationToken::new();
        let listener_token = token.clone();
        let server = tokio::spawn(start_raw_udp_socket(
            socket,
            b"udp response".to_vec(),
            listener_token,
            std::path::PathBuf::from("test"),
        ));
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(b"request", address).await.unwrap();
        let mut response = [0u8; 64];
        let (read, _) = client.recv_from(&mut response).await.unwrap();
        token.cancel();
        server.await.unwrap();

        assert_eq!(&response[..read], b"udp response");
    }
}
