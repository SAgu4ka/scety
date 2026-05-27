use std::collections::HashMap;
use std::sync::OnceLock;
use serde::Deserialize;

const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const TOML_SOURCE: &str = include_str!("../models/http_code_table/default.toml");

#[derive(Deserialize)]
struct RawStatusRegistry {
    statuses: HashMap<String, String>,
}

static STATUS_MAP: OnceLock<HashMap<u16, String>> = OnceLock::new();

pub fn get_status_message(code: u16) -> &'static str {
    let map = STATUS_MAP.get_or_init(|| {
        let registry: RawStatusRegistry = toml::from_str(TOML_SOURCE)
            .expect("Critical error: default.toml is invalid!");
        
        registry.statuses
            .into_iter()
            .filter_map(|(k, v)| {
                k.parse::<u16>().ok().map(|num| (num, v))
            })
            .collect()
    });

    map.get(&code)
        .map(|s| s.as_str())
        .unwrap_or("Something went wrong.")
}

pub async fn generate_text_response(
    code: u16,
    connection: &str,
    with_content: bool,
    content_type: Option<&str>,
    content: Option<&str>,
    expose_version: bool,
) -> String {
    let server_header = if expose_version {
        format!("{}/{}", ENGINE_NAME, ENGINE_VERSION)
    } else {
        ENGINE_NAME.to_string()
    };

    let mut response = format!(
        "HTTP/1.1 {} {}\r\nServer: {}\r\nConnection: {}\r\n",
        code,
        get_status_message(code),
        server_header,
        connection
    );

    if with_content {
        if let Some(content_type) = content_type {
            response.push_str(&format!("Content-Type: {}\r\n", content_type))
        }

        if let Some(body) = content {
            response.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
        } else {
            response.push_str("Content-Length: 0\r\n\r\n");
        }
    } else {
        response.push_str("\r\n");
    }

    response
}