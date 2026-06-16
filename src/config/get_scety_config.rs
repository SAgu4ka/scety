use crate::config::settings::SCETY_CONFIG_PATH;
use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::error;

pub static SCETY_CONFIG: OnceLock<ScetyConfig> = OnceLock::new();

#[derive(Deserialize)]
struct TomlConfig {
    limitation: LimitationSection,
}

#[derive(Deserialize)]
struct LimitationSection {
    ip_limitation: Option<i32>,
    client_timeout: Option<String>,
}

pub struct ScetyConfig {
    pub ip_limitation: Option<i32>,
    pub client_timeout: Option<Duration>,
}

impl ScetyConfig {
    pub fn new(ip_limitation: Option<i32>, client_timeout: Option<String>) -> Self {
        let ip_limitation = Some(ip_limitation.unwrap_or(20));
        let client_timeout = client_timeout.unwrap_or_else(|| "5s".to_string());

        let client_timeout_duration = humantime::parse_duration(&client_timeout).ok();

        Self {
            ip_limitation,
            client_timeout: client_timeout_duration,
        }
    }
}

pub fn get_scety_config() -> std::io::Result<Option<ScetyConfig>> {
    if !std::path::Path::new(SCETY_CONFIG_PATH).exists() {
        error!(path=%SCETY_CONFIG_PATH, "The main configuration file is missing");
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File scety.toml is missing!",
        ));
    }

    let content = match fs::read_to_string(SCETY_CONFIG_PATH) {
        Ok(c) => c,
        Err(e) => {
            error!(error=%e, file=%SCETY_CONFIG_PATH, "Failed to read configuration file");
            return Err(e);
        }
    };

    let toml_data: TomlConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, file = %SCETY_CONFIG_PATH, "Failed to parse configuration file");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            ));
        }
    };

    let config = ScetyConfig::new(
        toml_data.limitation.ip_limitation,
        toml_data.limitation.client_timeout,
    );

    Ok(Some(config))
}

pub fn scety_config() -> &'static ScetyConfig {
    SCETY_CONFIG
        .get()
        .expect("ScetyConfig is not initialized yet!")
}
