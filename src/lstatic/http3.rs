use super::config::{Files, StaticConfig, TransportProtocol};
use bytes::Bytes;
use quinn::{Endpoint, ServerConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub async fn start(port: u16, config: StaticConfig, token: CancellationToken) {
    let TransportProtocol::Http3 { ssl, files } = config.mode else {
        return;
    };
    let Some((cert_path, key_path)) = ssl.cert.as_deref().zip(ssl.key.as_deref()) else {
        error!(port, "HTTP/3 requires certificate and key");
        return;
    };
    let certs: Vec<CertificateDer<'static>> = match CertificateDer::pem_file_iter(cert_path)
        .and_then(|iter| iter.collect::<Result<Vec<_>, _>>())
    {
        Ok(certs) if !certs.is_empty() => certs,
        Ok(_) => {
            error!(port, "HTTP/3 certificate chain is empty");
            return;
        }
        Err(error) => {
            error!(%error, port, "HTTP/3 certificate load failed");
            return;
        }
    };
    let key = match PrivateKeyDer::from_pem_file(key_path) {
        Ok(key) => key,
        Err(error) => {
            error!(%error, port, "HTTP/3 private key load failed");
            return;
        }
    };
    let mut server_config = match ServerConfig::with_single_cert(certs, key) {
        Ok(config) => config,
        Err(error) => {
            error!(%error, port, "HTTP/3 TLS configuration failed");
            return;
        }
    };
    let mut transport = quinn::TransportConfig::default();
    transport.max_concurrent_bidi_streams(100u32.into());
    server_config.transport_config(Arc::new(transport));
    let endpoint = match Endpoint::server(server_config, ([0, 0, 0, 0], port).into()) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            error!(%error, port, "HTTP/3 endpoint bind failed");
            return;
        }
    };
    let files = Arc::new(files);
    info!(port, "HTTP/3 static listener started");

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                endpoint.close(0u32.into(), b"shutdown");
                break;
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(error) => { warn!(%error, port, "HTTP/3 connection failed"); continue; }
                };
                let files = Arc::clone(&files);
                tokio::spawn(async move { serve_connection(connection, files).await; });
            }
        }
    }
}

async fn serve_connection(connection: quinn::Connection, files: Arc<Files>) {
    let quic = h3_quinn::Connection::new(connection);
    let mut server = match h3::server::builder().build(quic).await {
        Ok(server) => server,
        Err(error) => {
            warn!(%error, "HTTP/3 session setup failed");
            return;
        }
    };
    while let Ok(Some(resolver)) = server.accept().await {
        let (request, mut stream) = match resolver.resolve_request().await {
            Ok(request) => request,
            Err(error) => {
                warn!(%error, "HTTP/3 request decode failed");
                continue;
            }
        };
        let Some((root, relative)) = resolve_file(&files, request.uri().path()) else {
            let response = http::Response::builder().status(404).body(()).unwrap();
            let _ = stream.send_response(response).await;
            let _ = stream.finish().await;
            continue;
        };
        let file = root.join(if relative.is_empty() {
            "index.html"
        } else {
            relative
        });
        let content = match tokio::fs::canonicalize(&file).await {
            Ok(file) if file.starts_with(root) => tokio::fs::read(file).await.ok(),
            _ => None,
        };
        let (status, body) = content.map_or((404, Bytes::from_static(b"Not Found")), |body| {
            (200, Bytes::from(body))
        });
        let response = http::Response::builder().status(status).body(()).unwrap();
        if stream.send_response(response).await.is_ok() && request.method() != http::Method::HEAD {
            let _ = stream.send_data(body).await;
        }
        let _ = stream.finish().await;
    }
}

fn resolve_file<'a>(files: &'a Files, request_path: &'a str) -> Option<(&'a Path, &'a str)> {
    match files {
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
    }
}
