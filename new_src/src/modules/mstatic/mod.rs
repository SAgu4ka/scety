use std::collections::HashMap;

use std::fs::read_to_string;

use crate::{
    DynResult, core::{ConfigValidationResult, Module}, modules::mstatic::config::{StaticConfig, validate_static_configs},
};

pub mod config;

pub struct StaticModule {}

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
            dyn ::core::future::Future<Output = crate::DynResult<()>>
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
