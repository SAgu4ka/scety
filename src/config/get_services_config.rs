use walkdir::WalkDir;
use std::path::PathBuf;
use crate::config::settings::SERVICES_CONFIGS_PATH;
use serde::Deserialize;
use std::fs;
use tracing::{info, debug, error};
use std::collections::HashMap;

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
    pub headers: Option<HeadersConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpstreamConfig {
    pub port: Option<u16>,
    pub ports: Option<Vec<u16>>,
    pub ip_limitation: Option<i32>,
    pub client_timeout: Option<String>,
    pub service_timeout: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SslConfig {
    pub cert: String,
    pub key: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeadersConfig {
    pub upstream: Option<HashMap<String, String>>,
    pub response: Option<HashMap<String, String>>,
}

fn get_all_config_paths()  -> Vec<PathBuf>{
    debug!("Starting searching for configuration files...");
    WalkDir::new(SERVICES_CONFIGS_PATH)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "toml"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn get_all_configs() -> Vec<ClientConfig> {
    let mut vec_confs = Vec::new();
    let all_configs = get_all_config_paths();
    info!("{} files found in the configuration directory. Running validation check...", all_configs.len());

    for conf in all_configs.iter() {

        let content = match fs::read_to_string(conf) {
            Ok(c) => c,
            Err(_) => { error!(file=%conf.to_string_lossy(), "Failed to read configuration file"); continue; }
        };

        let config: ClientConfig = match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => { error!(error=%e, file=%conf.to_string_lossy(), "Error parsing configuration file"); continue; }
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
        (None, None) => return Err("Either 'listen_port' or 'listens_port' must be specified".to_string()),
        (Some(_), Some(_)) => return Err("'listen_port' and 'listens_port' are mutually exclusive".to_string()),
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
            if key.starts_with("upstream_") {
                let port_str = &key["upstream_".len()..];
                if let Ok(port) = port_str.parse::<u16>() {
                    if !ports.contains(&port) {
                        return Err(format!(
                            "[{}] found, but port {} is not in listens_port={:?}. Maybe listens_port should include {}?",
                            key, port, ports, port
                        ));
                    }
                }
            }
        }
    } else if config.upstream.is_none() {
        return Err("Missing [upstream] section".to_string());
    }

    let check_upstream = |u: &UpstreamConfig| -> Result<(), String> {
        match (&u.port, &u.ports) {
            (None, None) => Err("Either 'port' or 'ports' must be specified in [upstream]".to_string()),
            (Some(_), Some(_)) => Err("'port' and 'ports' are mutually exclusive in [upstream]".to_string()),
            _ => Ok(())
        }
    };

    if let Some(upstream) = &config.upstream {
        check_upstream(upstream)?;
    }
    for (key, upstream) in &config.upstreams {
        check_upstream(upstream).map_err(|e| format!("[{}]: {}", key, e))?;
    }

    Ok(())
}