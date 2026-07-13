use crate::config::get_services_config::ClientConfig;
use std::fs;
use std::sync::Arc;
use tokio_rustls::rustls::RootCertStore;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tokio_rustls::rustls::client::danger::ServerCertVerifier;
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tracing::{error, warn};
use x509_parser::extensions::GeneralName;
use x509_parser::pem::Pem;
use x509_parser::prelude::{FromDer, X509Certificate};

const EXPIRY_WARNING_DAYS: i64 = 14;

pub fn check_all_configured_certs(configs: &[ClientConfig], extra_ca_bundle: Option<&str>) -> bool {
    let mut all_ok = true;

    for config in configs {
        let hosts = expected_hosts(config);
        let label = hosts.join(", ");

        let mut check_one = |ssl: &crate::config::get_services_config::SslConfig| {
            if ssl.acme.unwrap_or(false) {
                return;
            }
            if let Some(cert_path) = &ssl.cert
                && !check_certificate(&label, cert_path, &hosts, extra_ca_bundle)
            {
                all_ok = false;
            }
        };

        if let Some(ssl) = &config.ssl {
            check_one(ssl);
        }
        for ssl in config.ssl_ports.values() {
            check_one(ssl);
        }
    }

    all_ok
}

fn expected_hosts(config: &ClientConfig) -> Vec<String> {
    if let Some(host) = &config.host {
        vec![host.clone()]
    } else {
        config.hosts.clone().unwrap_or_default()
    }
}

pub fn check_certificate(
    label: &str,
    cert_path: &str,
    expected_hosts: &[String],
    extra_ca_bundle: Option<&str>,
) -> bool {
    let bytes = match fs::read(cert_path) {
        Ok(b) => b,
        Err(e) => {
            error!(cert=%label, path=%cert_path, error=%e, "Failed to read the certificate file");
            return false;
        }
    };

    let der_blocks: Vec<Vec<u8>> = match Pem::iter_from_buffer(&bytes)
        .map(|r| r.map(|pem| pem.contents))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => {
            error!(cert=%label, path=%cert_path, "No PEM certificate blocks were found in the file");
            return false;
        }
        Err(e) => {
            error!(cert=%label, path=%cert_path, error=%e, "Failed to parse the certificate PEM");
            return false;
        }
    };

    let leaf_der = &der_blocks[0];
    let cert = match X509Certificate::from_der(leaf_der) {
        Ok((_, c)) => c,
        Err(e) => {
            error!(cert=%label, path=%cert_path, error=%e, "Failed to parse X.509 certificate");
            return false;
        }
    };

    let mut ok = true;

    let now_ts = x509_parser::time::ASN1Time::now().timestamp();
    let not_after_ts = cert.validity().not_after.timestamp();
    if not_after_ts < now_ts {
        error!(
            cert = %label, path = %cert_path,
            not_after = %cert.validity().not_after,
            "Certificate expired"
        );
        ok = false;
    } else {
        let days_left = (not_after_ts - now_ts) / 86_400;
        if days_left <= EXPIRY_WARNING_DAYS {
            warn!(
                cert = %label, path = %cert_path, days_left = %days_left,
                "The certificate is about to expire"
            );
            ok = false;
        }
    }

    if cert.issuer().to_string() == cert.subject().to_string() {
        warn!(cert=%label, path=%cert_path, "Self-signed certificate (issuer == subject)");
        ok = false;
    }

    let sans = extract_dns_sans(&cert);
    if sans.is_empty() {
        warn!(
            cert = %label, path = %cert_path,
            "The certificate lacks a SAN (Subject Alternative Name), modern browsers may not accept such a certificate"
        );
        ok = false;
    } else {
        let matched = expected_hosts
            .iter()
            .any(|h| sans.iter().any(|s| domain_matches(s, h)));
        if !matched {
            warn!(
                cert = %label, path = %cert_path, sans = ?sans, expected = ?expected_hosts,
                "The certificate domains (SAN) do not match the domain(s) of the service for which it is configured"
            );
            ok = false;
        }
    }

    if let Err(e) = check_trust_chain(&der_blocks, expected_hosts, extra_ca_bundle) {
        warn!(cert=%label, path=%cert_path, error=%e, "The certificate chain is not validated by any trusted certificate authority");
        ok = false;
    }

    ok
}

fn domain_matches(san: &str, host: &str) -> bool {
    match san.strip_prefix("*.") {
        Some(rest) => match host.split_once('.') {
            Some((_, remainder)) => remainder.eq_ignore_ascii_case(rest),
            None => false,
        },
        None => san.eq_ignore_ascii_case(host),
    }
}

fn extract_dns_sans(cert: &X509Certificate) -> Vec<String> {
    match cert.subject_alternative_name() {
        Ok(Some(san)) => san
            .value
            .general_names
            .iter()
            .filter_map(|gn| match gn {
                GeneralName::DNSName(d) => Some(d.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn check_trust_chain(
    der_blocks: &[Vec<u8>],
    expected_hosts: &[String],
    extra_ca_bundle: Option<&str>,
) -> Result<(), String> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(bundle_path) = extra_ca_bundle {
        let bundle_bytes = fs::read(bundle_path)
            .map_err(|e| format!("failed to read trusted_ca_bundle {bundle_path}: {e}"))?;
        let mut reader = std::io::BufReader::new(bundle_bytes.as_slice());
        for cert in rustls_pemfile::certs(&mut reader) {
            let cert = cert.map_err(|e| format!("ошибка парсинга trusted_ca_bundle: {e}"))?;
            roots
                .add(cert)
                .map_err(|e| format!("failed to add certificate from trusted_ca_bundle: {e}"))?;
        }
    }

    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| format!("failed to build the chain verifier: {e}"))?;

    let end_entity = CertificateDer::from(der_blocks[0].clone());
    let intermediates: Vec<CertificateDer> = der_blocks[1..]
        .iter()
        .cloned()
        .map(CertificateDer::from)
        .collect();

    let host = expected_hosts
        .first()
        .ok_or("there are no domains for which the service is configured — nothing to check")?;
    let server_name = ServerName::try_from(host.clone())
        .map_err(|_| format!("invalid domain name in the config: {host}"))?;

    verifier
        .verify_server_cert(
            &end_entity,
            &intermediates,
            &server_name,
            &[],
            UnixTime::now(),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}
