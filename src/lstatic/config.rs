use crate::_core::SslConfig;
use toml::Value;

pub enum HttpVersion {
    Http09,
    Http1,
    Http2,
    Auto, // ALPN check
}

pub enum TransportProtocol {
    Tcp {
        ssl: Option<SslConfig>,
        http_version: HttpVersion
    },
    Http3 {
        ssl: SslConfig
    },
    RawUdp,
    RawTcp {
        ssl: Option<SslConfig>
    }
}

pub struct StaticConfig {
    pub name: String,
    pub port: u16,
    pub mode: TransportProtocol
}

