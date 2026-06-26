#![allow(unused)] // пока что заглушка нереализованных переменных, они задуманы на будущее

use crate::config::settings::SERVICES_CONFIGS_PATH;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info};
use walkdir::WalkDir;

#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    pub mode: String,
    pub host: Option<String>,
    pub hosts: Option<Vec<String>>,
    pub listen_port: Option<u16>,
    pub listens_port: Option<Vec<u16>>,
    pub upstream: Option<UpstreamConfig>,
    #[serde(flatten)]
    pub upstreams: HashMap<String, UpstreamConfig>,
    pub ssl: Option<SslConfig>,
    #[serde(flatten)]
    pub ssl_ports: HashMap<String, SslConfig>,
    pub headers: Option<HeadersConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamConfig {
    pub port: Option<u16>,
    pub ports: Option<Vec<u16>>,
    pub service_timeout: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SslConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub acme: Option<bool>,
    pub acme_email: Option<String>,
    pub acme_domains: Option<Vec<String>>,
    pub acme_cache: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeadersConfig {
    pub upstream: Option<HashMap<String, String>>,
    pub response: Option<HashMap<String, String>>,
}

fn get_all_config_paths() -> Vec<PathBuf> {
    debug!("Starting searching for configuration files...");
    WalkDir::new(SERVICES_CONFIGS_PATH)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn get_all_configs() -> Vec<ClientConfig> {
    let mut vec_confs = Vec::new();
    let all_configs = get_all_config_paths();
    info!(
        "{} files found in the configuration directory. Running validation check...",
        all_configs.len()
    );

    for conf in all_configs.iter() {
        let content = match fs::read_to_string(conf) {
            Ok(c) => c,
            Err(e) => {
                error!(error=%e, file=%conf.to_string_lossy(), "Failed to read configuration file");
                continue;
            }
        };

        let config: ClientConfig = match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                error!(error=%e, file=%conf.to_string_lossy(), "Error parsing configuration file");
                continue;
            }
        };

        if let Err(e) = validate_config(&config) {
            error!(error=%e, file=%conf.to_string_lossy(), "Configuration validation failed");
            continue;
        }

        debug!(file=%conf.to_string_lossy(), "Configuration file loaded successfully");
        vec_confs.push(config);
    }

    vec_confs
}

pub fn validate_config(config: &ClientConfig) -> Result<(), String> {
    match (&config.host, &config.hosts) {
        (None, None) => return Err("Either 'host' or 'hosts' must be specified".to_string()),
        (Some(_), Some(_)) => return Err("'host' and 'hosts' are mutually exclusive".to_string()),
        _ => {}
    }

    match (&config.listen_port, &config.listens_port) {
        (None, None) => {
            return Err("Either 'listen_port' or 'listens_port' must be specified".to_string());
        }
        (Some(_), Some(_)) => {
            return Err("'listen_port' and 'listens_port' are mutually exclusive".to_string());
        }
        _ => {}
    }

    if let Some(ports) = &config.listens_port {
        for port in ports {
            let key = format!("upstream_{}", port);
            if !config.upstreams.contains_key(&key) {
                return Err(format!("Missing [{}] section for port {}", key, port));
            }
        }
        for key in config.upstreams.keys() {
            if let Some(port_str) = key.strip_prefix("upstream_")
                && let Ok(port) = port_str.parse::<u16>()
                && !ports.contains(&port)
            {
                return Err(format!(
                    "[{}] found, but port {} is not in listens_port={:?}. Maybe listens_port should include {}?",
                    key, port, ports, port
                ));
            }
        }
    } else if config.upstream.is_none() {
        return Err("Missing [upstream] section".to_string());
    }

    let check_upstream = |u: &UpstreamConfig| -> Result<(), String> {
        match (&u.port, &u.ports) {
            (None, None) => {
                Err("Either 'port' or 'ports' must be specified in [upstream]".to_string())
            }
            (Some(_), Some(_)) => {
                Err("'port' and 'ports' are mutually exclusive in [upstream]".to_string())
            }
            _ => Ok(()),
        }
    };

    if let Some(upstream) = &config.upstream {
        check_upstream(upstream)?;
    }
    for (key, upstream) in &config.upstreams {
        check_upstream(upstream).map_err(|e| format!("[{}]: {}", key, e))?;
    }

    if let Some(ports) = &config.listens_port {
        for (key, ssl) in &config.ssl_ports {
            if let Some(port_str) = key.strip_prefix("ssl_") {
                if let Ok(port) = port_str.parse::<u16>() {
                    if !ports.contains(&port) {
                        return Err(format!(
                            "[{}] found, but port {} is not in listens_port={:?}",
                            key, port, ports
                        ));
                    }
                }
            }
        }
    }

    let check_ssl = |ssl: &SslConfig| -> Result<(), String> {
        let is_acme = ssl.acme.unwrap_or(false);
        if is_acme {
            if ssl.acme_email.is_none() {
                return Err("acme=true requires acme_email".to_string());
            }
            if ssl.acme_domains.is_none() {
                return Err("acme=true requires acme_domains".to_string());
            }
        } else {
            if ssl.cert.is_none() || ssl.key.is_none() {
                return Err("SSL requires either acme=true or both cert and key".to_string());
            }
        }
        Ok(())
    };

    if let Some(ssl) = &config.ssl {
        check_ssl(ssl)?;
    }
    for (key, ssl) in &config.ssl_ports {
        check_ssl(ssl).map_err(|e| format!("[{}]: {}", key, e))?;
    }

    Ok(())
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     fn base_config() -> ClientConfig {
//         ClientConfig {
//             mode: "proxy".to_string(),
//             host: Some("example.com".to_string()),
//             hosts: None,
//             listen_port: Some(80),
//             listens_port: None,
//             upstream: Some(UpstreamConfig {
//                 port: Some(3000),
//                 ports: None,
//                 service_timeout: None,
//             }),
//             upstreams: HashMap::new(),
//             ssl: None,
//             headers: None,
//         }
//     }

//     #[test]
//     fn valid_config_passes() {
//         assert!(validate_config(&base_config()).is_ok());
//     }

//     #[test]
//     fn missing_host_and_hosts_fails() {
//         let mut c = base_config();
//         c.host = None;
//         assert!(validate_config(&c).is_err());
//     }

//     #[test]
//     fn both_host_and_hosts_fails() {
//         let mut c = base_config();
//         c.hosts = Some(vec!["other.com".to_string()]);
//         assert!(validate_config(&c).is_err());
//     }

//     #[test]
//     fn missing_listen_port_fails() {
//         let mut c = base_config();
//         c.listen_port = None;
//         assert!(validate_config(&c).is_err());
//     }

//     #[test]
//     fn missing_upstream_fails() {
//         let mut c = base_config();
//         c.upstream = None;
//         assert!(validate_config(&c).is_err());
//     }

//     #[test]
//     fn upstream_with_both_port_and_ports_fails() {
//         let mut c = base_config();
//         c.upstream = Some(UpstreamConfig {
//             port: Some(3000),
//             ports: Some(vec![3001]),
//             service_timeout: None,
//         });
//         assert!(validate_config(&c).is_err());
//     }
// }
