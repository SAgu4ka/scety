use crate::core::request::HttpRequest;
use crate::core::kwargs::Kwargs;

pub fn parse_http_request(raw_buffer: &str) -> Option<HttpRequest<'_>> {
    let (headers_part, body_part) = raw_buffer.split_once("\r\n\r\n")?;

    let mut lines = headers_part.lines();

    let start_line = lines.next()?;
    let mut start_line_parts = start_line.split_whitespace();

    let method = start_line_parts.next()?;
    let path = start_line_parts.next()?;
    let version = start_line_parts.next()?;

    let host_header = lines.clone().find(|line| line.to_lowercase().starts_with("host:"))?;
    let host = host_header.split_once(":")?.1.trim();

    let mut headers = Kwargs::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(":") {
            headers.set(key.trim(), value.trim());
        }
    }

    let body = if!body_part.is_empty() {
        Some(body_part)
    } else {
        None
    };

    Some(HttpRequest {
        method,
        host,
        path,
        version,
        headers,
        body,
    })
}