use crate::_core::SslConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use toml::Value;
use tracing::{error, warn};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum HttpVersion {
    #[serde(rename = "http/0.9", alias = "http09")]
    Http09,
    #[serde(rename = "http/1.1", alias = "http1")]
    Http1,
    Http2,
    Auto,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RootDirConfig {
    pub path: PathBuf,
    #[serde(default = "default_index")]
    pub index_file: String,
    pub fallback_file: Option<PathBuf>,
}

fn default_index() -> String {
    "index.html".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Files {
    SimpleRootDir(PathBuf),
    RootDir(RootDirConfig),
    Map(HashMap<String, PathBuf>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum TransportProtocol {
    Tcp {
        ssl: Option<SslConfig>,
        http_version: HttpVersion,
        files: Files,
    },
    Http3 {
        ssl: SslConfig,
        files: Files,
    },
    RawUdp {
        path: PathBuf,
    },
    RawTcp {
        ssl: Option<SslConfig>,
        path: PathBuf,
    },
}

impl TransportProtocol {
    pub fn files(&self) -> Option<&Files> {
        match self {
            Self::Tcp { files, .. } | Self::Http3 { files, .. } => Some(files),
            _ => None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::RawUdp { path } | Self::RawTcp { path, .. } => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StaticConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub mode: TransportProtocol,
}

pub fn validate_static_configs(configs: &[StaticConfig]) -> (bool, u16) {
    let mut has_errors = false;
    let mut warnings: u16 = 0;
    let mut seen_names = HashMap::new();
    let mut seen_ports = HashMap::new();

    for (idx, config) in configs.iter().enumerate() {
        if let Some(prev_idx) = seen_names.insert(&config.name, idx) {
            error!(
                config_idx = idx,
                duplicate_name = %config.name,
                "Duplicate 'name' field found. Previously defined at index {}",
                prev_idx
            );
            has_errors = true;
        }

        if let Some(prev_idx) = seen_ports.insert(config.port, idx) {
            error!(
                config_idx = idx,
                duplicate_port = config.port,
                "Duplicate 'port' field found. Previously defined at index {}",
                prev_idx
            );
            has_errors = true;
        }

        if let Some(files) = config.mode.files() {
            let (paths_ok, path_warns) = check_files_exist(files, idx);
            warnings += path_warns;
            if !paths_ok {
                has_errors = true;
            }
        }

        if let Some(path) = config.mode.path()
            && !path.exists()
        {
            warn!(
                config_idx = idx,
                file_path = %path.display(),
                "Path does not exist for raw transport mode"
            );
            warnings += 1;
        }
    }

    (!has_errors, warnings)
}

fn check_files_exist(files: &Files, config_idx: usize) -> (bool, u16) {
    let mut has_errors = false;
    let mut warnings = 0;

    let check_path = |path: &Path, key_desc: &str| -> u16 {
        if !path.exists() {
            warn!(
                config_idx = config_idx,
                file_path = %path.display(),
                "File path does not exist for {}",
                key_desc
            );
            1
        } else {
            0
        }
    };

    match files {
        Files::SimpleRootDir(path) => {
            warnings += check_path(path, "root directory");
        }
        Files::RootDir(config) => {
            warnings += check_path(&config.path, "root directory path");
            if let Some(fallback) = &config.fallback_file {
                warnings += check_path(fallback, "fallback file");
            }
        }
        Files::Map(map) => {
            for (key, path) in map {
                if key.trim().is_empty() {
                    error!(
                        config_idx = config_idx,
                        "Empty key found in 'files' map. Keys must be non-empty strings."
                    );
                    has_errors = true;
                }
                warnings += check_path(path, &format!("key '{key}'"));
            }
        }
    }

    (!has_errors, warnings)
}

pub fn parse_static_configs(raw_configs: Vec<Value>) -> Vec<StaticConfig> {
    let mut parsed_configs = Vec::with_capacity(raw_configs.len());

    for (idx, raw_config) in raw_configs.into_iter().enumerate() {
        match raw_config.try_into::<StaticConfig>() {
            Ok(config) => parsed_configs.push(config),
            Err(e) => error!(config_idx = idx, "Failed to parse config: {}", e),
        }
    }

    let (valid, _warnings) = validate_static_configs(&parsed_configs);
    if !valid {
        error!("Static configuration validation failed. Please fix the errors and try again.");
        return Vec::new();
    }

    parsed_configs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_toml(toml_str: &str) -> Vec<Value> {
        let value: Value = toml::from_str(toml_str).unwrap();
        value
            .get("configs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn test_valid_tcp_config_parsing() {
        let toml_data = r#"
        [[configs]]
        name = "web_server"
        host = "127.0.0.1"
        port = 8080

        [configs.mode]
        protocol = "tcp"
        http_version = "auto"
        files = "/tmp"
        "#;

        let configs = parse_static_configs(parse_toml(toml_data));
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "web_server");
        assert_eq!(configs[0].port, 8080);
    }

    #[test]
    fn test_duplicate_name_and_port() {
        let toml_data = r#"
        [[configs]]
        name = "app"
        host = "127.0.0.1"
        port = 8080
        [configs.mode]
        protocol = "rawudp"
        path = "/tmp"

        [[configs]]
        name = "app" # duplicate name
        host = "127.0.0.1"
        port = 8080 # duplicate port
        [configs.mode]
        protocol = "rawudp"
        path = "/tmp"
        "#;

        let parsed = parse_static_configs(parse_toml(toml_data));
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_files_map_empty_key_error() {
        let toml_data = r#"
        [[configs]]
        name = "api"
        host = "0.0.0.0"
        port = 3000

        [configs.mode]
        protocol = "tcp"
        http_version = "http/1.1"

        [configs.mode.files]
        "" = "/tmp/test.txt" # Empty key
        "#;

        let parsed = parse_static_configs(parse_toml(toml_data));
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_non_existent_file_warning() {
        let non_existent_path = "/path/that/definitely/does/not/exist/123456789";

        let config = StaticConfig {
            name: "test_warn".to_string(),
            host: "127.0.0.1".to_string(),
            port: 9000,
            mode: TransportProtocol::Tcp {
                ssl: None,
                http_version: HttpVersion::Auto,
                files: Files::SimpleRootDir(PathBuf::from(non_existent_path)),
            },
        };

        let (valid, warnings) = validate_static_configs(&[config]);
        assert!(valid);
        assert_eq!(warnings, 1);
    }

    #[test]
    fn test_http_version_aliases() {
        let toml_data = r#"
        [[configs]]
        name = "v1"
        host = "127.0.0.1"
        port = 8001
        [configs.mode]
        protocol = "tcp"
        http_version = "http/0.9"
        files = "/tmp"

        [[configs]]
        name = "v2"
        host = "127.0.0.1"
        port = 8002
        [configs.mode]
        protocol = "tcp"
        http_version = "http09"
        files = "/tmp"
        "#;

        let raw = parse_toml(toml_data);
        let config1: StaticConfig = raw[0].clone().try_into().unwrap();
        let config2: StaticConfig = raw[1].clone().try_into().unwrap();

        assert!(matches!(
            config1.mode,
            TransportProtocol::Tcp {
                http_version: HttpVersion::Http09,
                ..
            }
        ));
        assert!(matches!(
            config2.mode,
            TransportProtocol::Tcp {
                http_version: HttpVersion::Http09,
                ..
            }
        ));
    }

    #[test]
    fn test_root_dir_config_defaults() {
        let toml_data = r#"
        [[configs]]
        name = "web"
        host = "127.0.0.1"
        port = 80

        [configs.mode]
        protocol = "tcp"
        http_version = "auto"

        [configs.mode.files]
        path = "/tmp"
        # index_file is omitted; default_index ("index.html") should be used
        "#;

        let raw = parse_toml(toml_data);
        let config: StaticConfig = raw[0].clone().try_into().unwrap();

        if let TransportProtocol::Tcp {
            files: Files::RootDir(root_cfg),
            ..
        } = config.mode
        {
            assert_eq!(root_cfg.index_file, "index.html");
            assert_eq!(root_cfg.path, PathBuf::from("/tmp"));
        } else {
            panic!("Expected Files::RootDir variant");
        }
    }
}
