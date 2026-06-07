use std::borrow::Cow;
use chrono::Utc;
use crate::http::generate_http_response::get_status_message;
use std::fmt::Write;
use tracing::debug;

const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct HttpResponse<'a> {
    pub code: u16,
    pub connection: String,
    pub expose_version: bool,
    pub headers: Vec<(String, String)>,
    pub body: Option<Cow<'a, str>>,
}

impl<'a> HttpResponse<'a> {
    pub fn new(code: u16, connection: &str, expose_version: bool) -> Self {
        Self {
            code,
            connection: connection.to_string(),
            expose_version,
            headers: Vec::with_capacity(8),
            body: None
        }
    }

    pub fn with_content(mut self, content_type: &str, body: impl Into<Cow<'a, str>>) -> Self {
        let body_cow = body.into();
        debug!(content_len=%body_cow.len(), "Adding Content to an HTTP Response");

        self.headers.push(("Content-Type".to_string(), content_type.to_string()));
        self.headers.push(("Content-Length".to_string(), body_cow.len().to_string()));

        self.body = Some(body_cow);
        self
    }

    pub fn to_http_string(&self) -> String {
        debug!("Converting an HTTP response to a string...");
        let body_len = self.body.as_ref().map_or(0, |b| b.len());
        let mut response_string = String::with_capacity(1024 + body_len);

        let code_message = get_status_message(self.code);

        let _ = write!(&mut response_string, "HTTP/1.1 {} {}\r\n", self.code, code_message);

        if self.expose_version {
            let _ = write!(&mut response_string, "Server: {}/{}\r\n", ENGINE_NAME, ENGINE_VERSION);
        } else {
            let _ = write!(&mut response_string, "Server: {}\r\n", ENGINE_NAME);
        };

        let _ = write!(&mut response_string, "Connection: {}\r\n", self.connection);

        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT");
        let _ = write!(&mut response_string, "Date: {}\r\n", date);

        for (key, value) in self.headers.iter() {
            let _ = write!(&mut response_string, "{}: {}\r\n", key, value);
        }

        let _ = write!(&mut response_string, "\r\n");

        if let Some(ref body) = self.body {
            response_string.push_str(body);
        }
        debug!(headers=%self.headers.len()+3, "Successfully converting an HTTP response to a string");
        response_string
    }
}
