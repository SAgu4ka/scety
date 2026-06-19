#![allow(warnings)]

use crate::config::get_services_config::ClientConfig;
use crate::core::HostRouter::{HostRouter, MatchKind};

pub struct SearchRouter {
    router: HostRouter,
    configs: Vec<ClientConfig>,
}

impl SearchRouter {
    pub fn new(configs: Vec<ClientConfig>) -> Self {
        let mut router = HostRouter::new();

        for (index, config) in configs.iter().enumerate() {
            if let Some(host) = &config.host {
                router.add_pattern(host);
            }
            if let Some(hosts) = &config.hosts {
                for host in hosts {
                    router.add_pattern(host);
                }
            }
        }

        Self { router, configs }
    }

    // pub fn find(&self, host: &str) -> Option<&ClientConfig> {
    //     let index = self.router.find(host)?;
    //     self.configs.get(index)
    // }
}
