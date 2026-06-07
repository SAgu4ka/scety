use walkdir::WalkDir;
use std::path::PathBuf;
use crate::config::settings::CONFIG_PATH;
use serde::Deserialize;
use std::fs;
use tracing::{info, debug, error};

#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    pub listen_port: u16,
    pub target_port: u16,
    pub host: String,
}

fn get_all_config_paths()  -> Vec<PathBuf>{
    debug!("Starting searching for configuration files...");
    WalkDir::new(CONFIG_PATH)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn get_all_configs() -> Vec<ClientConfig> {
    let mut vec_confs = Vec::new();
    let all_configs = get_all_config_paths();
    info!("{} files found in the configuration directory. Running validation check...", all_configs.len());
    for conf in all_configs.iter() {
        if let Ok(content) = fs::read_to_string(conf) {
            match toml::from_str::<ClientConfig>(&content) {
                Ok(config) => {
                    debug!(file=%conf.to_string_lossy(), "The configuration file has been processed and added successfully");
                    vec_confs.push(config);
                }
                Err(e) => {error!(error=%e, file=%conf.to_string_lossy(), "Error processing configuration file")}
            }
        } else {error!(file=%conf.to_string_lossy(), "Failed to read configuration file")}
    }

    vec_confs
}