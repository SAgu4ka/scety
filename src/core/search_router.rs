use crate::config::get_services_config::ClientConfig;
use crate::core::host_router::HostRouter;

pub struct SearchRouter {
    router: HostRouter,
    configs: Vec<ClientConfig>,
}

impl SearchRouter {
    pub fn new(configs: Vec<ClientConfig>) -> Self {
        let mut router = HostRouter::new();

        for (index, config) in configs.iter().enumerate() {
            if let Some(host) = &config.host {
                router.add_pattern(host, index);
            }
            if let Some(hosts) = &config.hosts {
                for host in hosts {
                    router.add_pattern(host, index);
                }
            }
        }

        Self { router, configs }
    }

    pub fn find(&self, host: &str) -> Option<&ClientConfig> {
        let index = self.router.matches(host)?;
        self.configs.get(index)
    }
}
