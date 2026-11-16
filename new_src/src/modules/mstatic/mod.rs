use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
use std::fs::read_to_string;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};
use crate::modules::mstatic::config::SslConfig;
use crate::{
    DynResult,
    core::{ConfigValidationResult, Module},
    modules::mstatic::config::{StaticConfig, TransportProtocol, validate_static_configs},
};

pub mod config;

pub struct StaticModule {}

impl StaticModule {
    pub fn new() -> Self {
        Self {  }
    }

    async fn start_port(&self, port: u16, configs: Vec<StaticConfig>, token: CancellationToken) {
        if configs.len() == 1 {
            match &configs[0].mode {
                TransportProtocol::RawTcp { ssl, path } => {
                    self.start_raw_tcp_port(port, ssl, path, token);
                    return;
                }
                TransportProtocol::RawUdp { path } => {
                    self.start_raw_udp_port(port, path, token).await;
                    return;
                }
                TransportProtocol::Http3 { ssl, files } => {
                    todo!()
                }
                TransportProtocol::Tcp { ssl, http_verxion, files } => {
                    todo!()
                }
            }
        }
    }

    async fn start_raw_tcp_port(&self, port: u16, ssl: &Option<SslConfig>, path: &PathBuf, token: CancellationToken) {
        if let Some(ssl) = ssl {
            self.start_tls_raw_tcp_port(port, path, ssl, token).await;
            return;
        }
        // compile_error!("Unimplemented variable below");
        let ip = "0.0.0.0";
        let listener = match TcpListener::bind((ip, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, bind=ip, port, "Raw TCP static listener bind failed");
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

        info!(bind=ip, port, path = %path.display(), "Raw TCP static listener started");
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

    async fn start_tls_raw_tcp_port(&self, port: u16, path: &PathBuf, ssl: &SslConfig, token: CancellationToken) {
        let Some(acceptor) = self.load_tls_acceptor(&ssl) else {
            if ssl.acme.unwrap_or(false) {
                error!(port, "ACME is not supported for raw TCP static mode");
            }
            return;
        };
        // compile_error!("Unimplemented variable below");
        let ip = "0.0.0.0";
        let listener = match TcpListener::bind((ip, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, bind=ip, port, "TLS raw TCP static listener bind failed");
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
                        Err(error) => error!(%error, bind=ip, port, "TLS raw TCP static accept failed"),
                    }
                }
            }
    }

    fn load_tls_acceptor(&self, ssl: &SslConfig) -> Option<TlsAcceptor> {
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

    async fn start_raw_udp_port(&self, port: u16, path: &PathBuf, token: CancellationToken) {
        // compile_error!("Unimplemented variable below");
        let ip = "0.0.0.0";
        let socket = match tokio::net::UdpSocket::bind((ip, port)).await {
            Ok(socket) => socket,
            Err(error) => {
                error!(%error, bind=ip, port, "Raw UDP static listener bind failed");
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
        self.start_raw_udp_socket(socket, content, token, path).await;
    }

    async fn start_raw_udp_socket(
        &self,
        socket: tokio::net::UdpSocket,
        content: Vec<u8>,
        token: CancellationToken,
        path: &PathBuf,
    ) {
        let mut request = [0u8; 65536];
        info!(address=?socket.local_addr(), path=?path, "Raw UDP static listener started");
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
}

impl Module for StaticModule {
    fn config_type_name(&self) -> &str {
        "static"
    }

    fn check_config(&self, configs: &[std::path::PathBuf]) -> ConfigValidationResult {
        let mut configs_for_check = HashMap::new();
        for config in configs {
            let content: StaticConfig =
                toml::from_str(&read_to_string(config).expect("Failed to read file"))
                    .expect("Failed parse toml file");
            configs_for_check.insert(content, config.clone());
        }
        validate_static_configs(configs_for_check)
    }

    fn run<'life0, 'async_trait>(
        &'life0 mut self,
        token: tokio_util::sync::CancellationToken,
        configs: Vec<std::path::PathBuf>,
        reload_rx: tokio::sync::mpsc::Receiver<Vec<std::path::PathBuf>>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = DynResult<()>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }
}
