use crate::_core::SslConfig;

struct L7ServiceConfig {
    pub name: String,
    pub listen_port: u16,
    pub ssl_config: Option<SslConfig>,
}
