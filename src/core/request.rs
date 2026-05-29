use crate::core::kwargs::Kwargs;

#[derive(Debug)]
pub struct HttpRequest<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub version: &'a str,
    pub headers: Kwargs<'a>,
    pub body: Option<&'a str>,
}