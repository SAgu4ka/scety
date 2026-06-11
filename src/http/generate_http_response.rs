use crate::core::response::HttpResponse;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const TOML_SOURCE: &str = include_str!("../models/http_code_table/default.toml");

#[derive(Deserialize)]
struct RawStatusRegistry {
    statuses: HashMap<String, String>,
}

static STATUS_MAP: OnceLock<HashMap<u16, String>> = OnceLock::new();

pub fn get_status_message(code: u16) -> &'static str {
    let map = STATUS_MAP.get_or_init(|| {
        let registry: RawStatusRegistry =
            toml::from_str(TOML_SOURCE).expect("Critical error: default.toml is invalid!");

        registry
            .statuses
            .into_iter()
            .filter_map(|(k, v)| k.parse::<u16>().ok().map(|num| (num, v)))
            .collect()
    });

    map.get(&code)
        .map(|s| s.as_str())
        .unwrap_or("Something went wrong.")
}

pub fn generate_text_response(
    code: u16,
    connection: &str,
    with_content: bool,
    content_type: Option<&str>,
    content: Option<&str>,
    expose_version: bool,
) -> String {
    let mut response = HttpResponse::new(code, connection, expose_version);

    if with_content {
        let c_type = content_type.unwrap_or("text/plain; charset=utf-8");
        let body = content.unwrap_or("");

        response = response.with_content(c_type, body);
    }

    response.to_http_string()
}
