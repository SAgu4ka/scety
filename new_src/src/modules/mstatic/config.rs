use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};
use tracing::{error, warn};

use crate::core::ConfigValidationResult;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct SslConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub acme: Option<bool>,
    pub acme_email: Option<String>,
    pub acme_domains: Option<Vec<String>>,
    pub acme_cache: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum HttpVersion {
    #[serde(rename = "http/0.9", alias = "http09")]
    Http09,
    #[serde(rename = "http/1.1", alias = "http1")]
    Http1,
    #[serde(rename = "http/2.0", alias = "http2")]
    Http2,
    Auto,
}

#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct RootDirConfig {
    pub path: PathBuf,
    #[serde_inline_default("index.html".to_string())]
    pub index_file: String,
    pub fallback_file: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum Files {
    SimpleRootDir(PathBuf),
    RootDir(RootDirConfig),
    Map(BTreeMap<String, PathBuf>),
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum TransportProtocol {
    Tcp {
        ssl: Option<SslConfig>,
        http_verxion: HttpVersion,
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

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct StaticConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub mode: TransportProtocol,
}

pub fn validate_static_configs(configs: HashMap<StaticConfig, PathBuf>) -> ConfigValidationResult {
    let mut has_errors = false;
    let mut seen_names = HashMap::new();
    let mut seen_ports = HashMap::new();
    let mut ports = Vec::new();
    let mut problem_files = HashSet::new();

    for (idx, (config, path_config)) in configs.iter().enumerate() {
        if let Some(prev_idx) = seen_names.insert(&config.name, idx) {
            error!(
                config_idx = idx,
                duplicate_name = %config.name,
                "Duplicate 'name' field found. Previously defined at index {}",
                prev_idx
            );
            has_errors = true;
            problem_files.insert(path_config);
        }

        ports.push(config.port);
        if let Some(prev_idx) = seen_ports.insert(config.port, idx) {
            error!(
                config_idx = idx,
                duplicate_port = config.port,
                "Duplicate 'port' field found. Previously defined at index {}",
                prev_idx
            );
            has_errors = true;
            problem_files.insert(path_config);
        }

        if let Some(files) = config.mode.files() {
            let (paths_ok, _) = check_files_exist(files, idx);
            if !paths_ok {
                has_errors = true;
                problem_files.insert(path_config);
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
            problem_files.insert(path_config);
        }

        let acme_unsupported = match &config.mode {
            TransportProtocol::Http3 { ssl, .. }
            | TransportProtocol::RawTcp { ssl: Some(ssl), .. } => ssl.acme.unwrap_or(false),
            _ => false,
        };
        if acme_unsupported {
            error!(
                config_idx = idx,
                service = %config.name,
                "ACME is unsupported for this static transport mode"
            );
            has_errors = true;
            problem_files.insert(path_config);
        }
    }

    ConfigValidationResult {
        success: has_errors,
        ports,
        problem_files: Some(problem_files.into_iter().cloned().collect()),
    }
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
