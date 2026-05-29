use walkdir::WalkDir;
use std::path::PathBuf;
use crate::config::settings::CONFIG_PATH;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    pub listen_port: u16,
    pub target_port: u16,
}

fn get_all_config_paths()  -> Vec<PathBuf>{
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

    for conf in all_configs.iter() {
        if let Ok(content) = fs::read_to_string(conf) {
            match toml::from_str::<ClientConfig>(&content) {
                Ok(config) => {
                    vec_confs.push(config);
                }
                Err(_e) => {}
            }
        } else {}
    }

    vec_confs
}