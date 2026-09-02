use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub acme: Option<bool>,
    pub acme_email: Option<String>,
    pub acme_domains: Option<Vec<String>>,
    pub acme_cache: Option<String>,
}
