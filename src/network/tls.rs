use crate::config::get_services_config::SslConfig;
use crate::config::settings::MAIN_SCETY_PATH;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use tracing::info;

pub fn load_manual_tls(ssl: &SslConfig) -> Result<TlsAcceptor, Box<dyn std::error::Error>> {
    let cert_path = ssl.cert.as_ref().ok_or("Missing cert path")?;
    let key_path = ssl.key.as_ref().ok_or("Missing key path")?;

    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    let file = File::open(Path::new(path))?;
    let mut reader = BufReader::new(file);
    let result: Result<Vec<_>, _> = certs(&mut reader).collect();
    let certs = result?;
    if certs.is_empty() {
        return Err(format!("No certificates found in {}", path).into());
    }
    info!(path=%path, count=%certs.len(), "Loaded TLS certificates");
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error>> {
    let file = File::open(Path::new(path))?;
    let mut reader = BufReader::new(file);
    let result: Result<Vec<_>, _> = pkcs8_private_keys(&mut reader).collect();
    let key = result?
        .into_iter()
        .next()
        .ok_or_else(|| format!("No private key found in {}", path))?;
    info!(path=%path, "Loaded TLS private key");
    Ok(PrivateKeyDer::Pkcs8(key))
}

pub fn build_acme_config(
    ssl: &SslConfig,
) -> tokio_rustls_acme::AcmeConfig<std::io::Error, std::io::Error> {
    let email = ssl.acme_email.clone().unwrap_or_default();
    let domains = ssl.acme_domains.clone().unwrap_or_default();
    let cache = ssl
        .acme_cache
        .clone()
        .unwrap_or_else(|| "{main_path}/acme-cache".replace("{main_path}", MAIN_SCETY_PATH).to_string());

    info!(domains=?domains, email=%email, cache=%cache, "Setting up ACME");

    tokio_rustls_acme::AcmeConfig::new(domains)
        .contact_push(format!("mailto:{}", email))
        .cache(tokio_rustls_acme::caches::DirCache::new(cache))
}
