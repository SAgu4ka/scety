use crate::config::settings::SCETY_CONFIG_PATH;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{error, warn};

pub static SCETY_CONFIG: OnceLock<ScetyConfig> = OnceLock::new();

#[derive(Deserialize)]
struct TomlConfig {
    limitation: Option<LimitationSection>,
    limit_buffers: Option<LimitBuffersSection>,
    tls: Option<TlsSection>,
    headers: Option<GlobalHeadersSection>,
}

#[derive(Deserialize, Default)]
struct TlsSection {
    trusted_ca_bundle: Option<String>,
}

#[derive(Deserialize, Default)]
struct GlobalHeadersSection {
    upstream: Option<HashMap<String, String>>,
    response: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct LimitationSection {
    ip_limitation: Option<i32>,
    client_headers_timeout: Option<String>,
    client_body_timeout: Option<String>,
    client_full_timeout: Option<String>,
}

#[derive(Deserialize, Default)]
struct LimitBuffersSection {
    client_header: Option<i32>,
}

pub struct ScetyConfig {
    pub ip_limitation: Option<i32>,
    pub client_headers_timeout: Option<Duration>,
    pub client_body_timeout: Option<Duration>,
    pub client_full_timeout: Option<Duration>,
    pub client_use_full_timeout: bool,
    pub client_header_buffer: Option<i32>,
    pub trusted_ca_bundle: Option<String>,
    pub global_upstream_headers: HashMap<String, String>,
    pub global_response_headers: HashMap<String, String>,
}

impl ScetyConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ip_limitation: Option<i32>,
        client_headers_timeout: Option<String>,
        client_body_timeout: Option<String>,
        client_full_timeout: Option<String>,
        client_header_buffer: Option<i32>,
        trusted_ca_bundle: Option<String>,
        global_upstream_headers: HashMap<String, String>,
        global_response_headers: HashMap<String, String>,
    ) -> Self {
        const DEFAULT: Duration = Duration::from_secs(5);

        let ip_limitation = Some(ip_limitation.unwrap_or(20));
        let client_header_buffer = Some(client_header_buffer.unwrap_or(16 * 1024));

        let headers_raw =
            Self::parse_optional_timeout(client_headers_timeout, "client_headers_timeout");
        let body_raw = Self::parse_optional_timeout(client_body_timeout, "client_body_timeout");
        let full_raw = Self::parse_optional_timeout(client_full_timeout, "client_full_timeout");

        let (final_headers, final_body, use_full_timeout) = match (headers_raw, body_raw, full_raw)
        {
            (Some(h), Some(b), full) => {
                if full.is_some() {
                    warn!(
                        "client_full_timeout is ignored: client_headers_timeout and client_body_timeout are set"
                    );
                }
                (h, b, false)
            }
            (Some(h), None, full) => {
                if full.is_some() {
                    warn!("client_full_timeout is ignored: client_headers_timeout is set");
                }
                (h, Some(DEFAULT), false)
            }
            (None, Some(b), full) => {
                if full.is_some() {
                    warn!("client_full_timeout is ignored: client_body_timeout is set");
                }
                (Some(DEFAULT), b, false)
            }
            (None, None, Some(f)) => (f, f, true),
            (None, None, None) => (Some(DEFAULT), Some(DEFAULT), false),
        };

        Self {
            ip_limitation,
            client_headers_timeout: final_headers,
            client_body_timeout: final_body,
            client_full_timeout: full_raw.flatten(),
            client_use_full_timeout: use_full_timeout,
            client_header_buffer,
            trusted_ca_bundle,
            global_upstream_headers,
            global_response_headers,
        }
    }

    fn parse_optional_timeout(raw: Option<String>, field_name: &str) -> Option<Option<Duration>> {
        let raw = raw?;
        match raw.trim() {
            "-1" => Some(None),
            other => match humantime::parse_duration(other) {
                Ok(d) => Some(Some(d)),
                Err(e) => {
                    warn!(error=%e, field=%field_name, value=%other, "Invalid timeout value, using 5s");
                    Some(Some(Duration::from_secs(5)))
                }
            },
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

    let limitation = toml_data.limitation.unwrap_or_default();
    let limit_buffers = toml_data.limit_buffers.unwrap_or_default();
    let tls = toml_data.tls.unwrap_or_default();
    let headers = toml_data.headers.unwrap_or_default();

    let config = ScetyConfig::new(
        limitation.ip_limitation,
        limitation.client_headers_timeout,
        limitation.client_body_timeout,
        limitation.client_full_timeout,
        limit_buffers.client_header,
        tls.trusted_ca_bundle,
        headers.upstream.unwrap_or_default(),
        headers.response.unwrap_or_default(),
    );

    Ok(Some(config))
}

pub fn scety_config() -> &'static ScetyConfig {
    SCETY_CONFIG
        .get()
        .expect("ScetyConfig is not initialized yet!")
}
