use crate::modules::mstatic::{StaticModuleInner, config::SslConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

impl StaticModuleInner {
    pub async fn start_raw_tcp_port(
        &self,
        port: u16,
        ssl: &Option<SslConfig>,
        path: &PathBuf,
        token: CancellationToken,
    ) {
        if let Some(ssl) = ssl {
            self.start_tls_raw_tcp_port(port, path, ssl, token).await;
            return;
        }
        let ip = self.get_ip();
        let listener = match TcpListener::bind((ip, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, bind=%ip, port, "Raw TCP static listener bind failed");
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

        info!(bind=%ip, port, path = %path.display(), "Raw TCP static listener started");
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
        &self,
        port: u16,
        path: &PathBuf,
        ssl: &SslConfig,
        token: CancellationToken,
    ) {
        let Some(acceptor) = self.load_tls_acceptor(&ssl) else {
            if ssl.acme.unwrap_or(false) {
                error!(port, "ACME is not supported for raw TCP static mode");
            }
            return;
        };
        let ip = self.get_ip();
        let listener = match TcpListener::bind((ip, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, bind=%ip, port, "TLS raw TCP static listener bind failed");
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
                    Err(error) => error!(%error, bind=%ip, port, "TLS raw TCP static accept failed"),
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
}
