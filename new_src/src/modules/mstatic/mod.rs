use crate::modules::mstatic::read_configs::read_files;
use crate::{
    DynResult,
    core::{ConfigValidationResult, Module},
    modules::mstatic::config::{StaticConfig, TransportProtocol, validate_static_configs},
};
use std::collections::HashMap;
use std::fs::read_to_string;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub mod config;
pub mod listeners;
pub mod read_configs;

pub struct StaticModule {
    inner: Arc<StaticModuleInner>,
    configs: Vec<StaticConfig>,
}

pub struct StaticModuleInner {
    ip: IpAddr
}

impl StaticModuleInner {
    pub fn new(ip: IpAddr) -> Self {
        Self {ip}
    }

    pub async fn start_port(
        &self,
        port: u16,
        configs: Vec<StaticConfig>,
        token: CancellationToken,
    ) {
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
                    self.start_http3_port(port, ssl, files, token).await;
                    return;
                }
                TransportProtocol::Tcp {
                    ssl,
                    http_version,
                    files,
                } => {
                    todo!();
                    self.start_tcp_port(port, ssl, files, http_version, token);
                    return;
                }
            }
        } else {
            todo!()
        }
    }

    pub fn get_ip (&self) -> IpAddr {
        self.ip
    }
}

impl StaticModule {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(StaticModuleInner::new([0, 0, 0, 0].into())),
            configs: Vec::new(),
        }
    }

    async fn prestart(&mut self, paths: Vec<PathBuf>) {
        let contents = read_files(paths).unwrap();
        for content in contents {
            self.configs
                .push(toml::from_slice(&content).expect("Failed parse toml file"));
        }
    }

    async fn start(&self, token: CancellationToken) {
        if self.configs.is_empty() {
            warn!("No valid static configurations remain after validation");
            token.cancelled().await;
            return;
        }
        let mut by_port: HashMap<u16, Vec<StaticConfig>> = HashMap::new();
        for config in &self.configs {
            by_port.entry(config.port).or_default().push(config.clone());
        }

        let mut listeners = JoinSet::new();
        for (port, configs) in by_port {
            let listener_token = token.child_token();
            let inner = Arc::clone(&self.inner);
            listeners.spawn(async move { inner.start_port(port, configs, listener_token).await });
        }
        while listeners.join_next().await.is_some() {}
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
        token: CancellationToken,
        configs: Vec<PathBuf>,
        reload_rx: tokio::sync::mpsc::Receiver<Vec<PathBuf>>,
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
        todo!();
        // self.prestart(configs).await;
        // self.start(token).await;
    }
}
