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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::get_services_config::{ClientConfig, UpstreamConfig};
    use std::collections::HashMap;

    fn make_config(host: &str, upstream_port: u16) -> ClientConfig {
        ClientConfig {
            mode: "proxy".to_string(),
            host: Some(host.to_string()),
            hosts: None,
            listen_port: Some(80),
            listens_port: None,
            upstream: Some(UpstreamConfig {
                port: Some(upstream_port),
                ports: None,
                service_timeout: None,
            }),
            upstreams: HashMap::new(),
            ssl: None,
            headers: None,
        }
    }

    #[test]
    fn finds_correct_upstream() {
        let router = SearchRouter::new(vec![
            make_config("api.example.com", 3000),
            make_config("web.example.com", 4000),
        ]);

        let cfg = router.find("api.example.com").unwrap();
        assert_eq!(cfg.upstream.as_ref().unwrap().port, Some(3000));
    }

    #[test]
    fn returns_none_for_unknown_host() {
        let router = SearchRouter::new(vec![make_config("example.com", 3000)]);
        assert!(router.find("unknown.com").is_none());
    }
}
