use std::borrow::Cow;
use chrono::Utc;
use crate::core::kwargs::Kwargs;
use crate::http::generate_http_response::get_status_message;

const ENGINE_NAME: &str = env!("CARGO_PKG_NAME");
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct HttpResponse<'a> {
    pub code: u16,
    pub connection: String,
    pub expose_version: bool,
    pub headers: Kwargs<'a>,
    pub body: Option<Cow<'a, str>>,
}

impl<'a> HttpResponse<'a> {
    pub fn new(code: u16, connection: &str, expose_version: bool) -> Self {
        Self {
            code,
            connection: connection.to_string(),
            expose_version,
            headers: Kwargs::new(),
            body: None
        }
    }

    pub fn with_content(mut self, content_type: &'a str, body: impl Into<Cow<'a, str>>) -> Self {
        let body_cow = body.into();
        self.headers.set("Content-Type", content_type);
        self.headers.set("Content-Length", body_cow.len().to_string());
        self.body = Some(body_cow);
        self
    }

    pub fn to_http_string(&self) -> String {
        let server_header = if self.expose_version {
            format!("{}/{}", ENGINE_NAME, ENGINE_VERSION)
        } else {
            ENGINE_NAME.to_string()
        };

        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let code_message = get_status_message(self.code);

        let mut response_string = format!(
            "HTTP/1.1 {} {}\r\nServer: {}\r\nConnection: {}\r\nDate: {}\r\n",
            self.code, code_message, server_header, self.connection, date
        );

        for (key, value) in self.headers.iter() {
            response_string.push_str(&format!("{}: {}\r\n", key, value));
        }

        response_string.push_str("\r\n");

        if let Some(ref body) = self.body {
            response_string.push_str(body);
        }

        response_string
    }
}
