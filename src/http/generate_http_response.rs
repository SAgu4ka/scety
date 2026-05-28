use std::collections::HashMap;
use std::sync::OnceLock;
use serde::Deserialize;
use chrono::Utc;
use crate::core::kwargs::Kwargs;

const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const TOML_SOURCE: &str = include_str!("../models/http_code_table/default.toml");

#[derive(Deserialize)]
struct RawStatusRegistry {
    statuses: HashMap<String, String>,
}

static STATUS_MAP: OnceLock<HashMap<u16, String>> = OnceLock::new();

pub fn generate_http_header(
    code: u16,
    code_message: &str,
    expose_version: bool,
    connection: &str,
    other_headers: Kwargs,
) -> String {
    let server_header = if expose_version {
        format!("{}/{}", ENGINE_NAME, ENGINE_VERSION)
    } else {
        ENGINE_NAME.to_string()
    };

    let data = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    let mut http_header = format!(
        "HTTP/1.1 {} {}\r\nServer: {}\r\nConnection: {}\r\nDate: {}\r\n",
        code,
        code_message, 
        server_header, 
        connection,
        data,
    );

    for (key, value) in other_headers.iter() {
        http_header.push_str(&format!("{}: {}\r\n", key, value));
    }

    http_header
}

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

    let mut content_header = Kwargs::new();

    if with_content {
        if let Some(content_type) = content_type {
            content_header.set("Content-Type", content_type);
        }

        if let Some(body) = content {
            content_header.set("Content-Length", body.len().to_string());
        } else {
            content_header.set("Content-Length", "0");
        }
    }

    let mut response = generate_http_header(code, get_status_message(code), expose_version, connection, content_header);

    if with_content {
        if let Some(content) = content {
            response.push_str(&format!("\r\n{}", content));
        } else {
            response.push_str("\r\n");
        }
    }else {
        response.push_str("\r\n");
    }

    response
}

