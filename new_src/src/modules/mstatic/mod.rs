use std::{collections::HashMap};
use std::fs::read_to_string;
use tokio_util::sync::CancellationToken;
use crate::{
    DynResult,
    core::{ConfigValidationResult, Module},
    modules::mstatic::config::{StaticConfig, TransportProtocol, validate_static_configs},
};

pub mod config;
pub mod listeners;

pub struct StaticModule {}

impl StaticModule {
    pub fn new() -> Self {
        Self {  }
    }

    async fn start_port(&self, port: u16, configs: Vec<StaticConfig>, token: CancellationToken) {
        if configs.len() == 1 {
            match &configs[0].mode {
                TransportProtocol::RawTcp { ssl, path } => {
                    self.start_raw_tcp_port(port, ssl, path, token).await;
                    return;
                }
                TransportProtocol::RawUdp { path } => {
                    self.start_raw_udp_port(port, path, token).await;
                    return;
                }
                TransportProtocol::Http3 { ssl, files } => {
                    todo!()
                }
                TransportProtocol::Tcp { ssl, http_version, files } => {
                    self.start_tcp_port(port, ssl, files, http_version, token);
                    return;
                }
            }
        }
    }


}

impl Module for StaticModule {
    fn config_type_name(&self) -> &str {
        "static"
    }

    fn check_config(&self, configs: &[std::path::PathBuf]) -> ConfigValidationResult {
        let mut configs_for_check = HashMap::new();
        for config in configs {
            let content: StaticConfig =
                toml::from_str(&read_to_string(config).expect("Failed to read file"))
                    .expect("Failed parse toml file");
            configs_for_check.insert(content, config.clone());
        }
        validate_static_configs(configs_for_check)
    }

    fn run<'life0, 'async_trait>(
        &'life0 mut self,
        token: tokio_util::sync::CancellationToken,
        configs: Vec<std::path::PathBuf>,
        reload_rx: tokio::sync::mpsc::Receiver<Vec<std::path::PathBuf>>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = DynResult<()>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }
}
