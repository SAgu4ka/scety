use crate::config::get_services_config::ClientConfig;
use crate::core::host_router::HostRouter;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub struct SearchRouter {
    router: HostRouter,
    configs: Vec<ClientConfig>,
    cache: Mutex<HashMap<String, Option<usize>>>,
    cache_order: Mutex<VecDeque<String>>,
    cache_capacity: usize,
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

        Self {
            router,
            configs,
            cache: Mutex::new(HashMap::new()),
            cache_order: Mutex::new(VecDeque::new()),
            cache_capacity: 128,
        }
    }

    pub fn find(&self, host: &str) -> Option<&ClientConfig> {
        self.find_with_cache(host)
    }

    fn find_with_cache(&self, host: &str) -> Option<&ClientConfig> {
        {
            let cache = self.cache.lock().expect("cache lock poisoned");
            let mut order = self.cache_order.lock().expect("cache order lock poisoned");
            if let Some(cached_index) = cache.get(host).copied() {
                self.touch_order(host, &mut order);
                return cached_index.and_then(|index| self.configs.get(index));
            }
        }

        let resolved_index = self.router.matches(host);
        {
            let mut cache = self.cache.lock().expect("cache lock poisoned");
            let mut order = self.cache_order.lock().expect("cache order lock poisoned");
            self.insert_cache_entry(host, resolved_index, &mut cache, &mut order);
        }

        resolved_index.and_then(|index| self.configs.get(index))
    }

    fn touch_order(&self, host: &str, order: &mut VecDeque<String>) {
        if let Some(pos) = order.iter().position(|entry| entry == host) {
            order.remove(pos);
        }
        order.push_back(host.to_string());
    }

    fn insert_cache_entry(
        &self,
        host: &str,
        index: Option<usize>,
        cache: &mut HashMap<String, Option<usize>>,
        order: &mut VecDeque<String>,
    ) {
        if cache.contains_key(host) {
            self.touch_order(host, order);
        } else {
            if cache.len() >= self.cache_capacity
                && let Some(oldest) = order.pop_front()
            {
                cache.remove(&oldest);
            }

            order.push_back(host.to_string());
        }

        cache.insert(host.to_string(), index);
    }
}

impl SearchRouter {
    #[allow(dead_code)]
    pub fn with_cache_capacity(mut self, capacity: usize) -> Self {
        self.cache_capacity = capacity;
        self
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
            ssl_ports: HashMap::new(),
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

    #[test]
    fn caches_successful_and_failed_lookups_with_lru_policy() {
        let router = SearchRouter::new(vec![
            make_config("example.com", 3000),
            make_config("api.example.com", 4000),
        ]);

        assert!(router.find("missing.example.com").is_none());
        assert_eq!(
            router
                .find("example.com")
                .unwrap()
                .upstream
                .as_ref()
                .unwrap()
                .port,
            Some(3000)
        );
        assert_eq!(
            router
                .find("api.example.com")
                .unwrap()
                .upstream
                .as_ref()
                .unwrap()
                .port,
            Some(4000)
        );
        assert!(router.find("missing.example.com").is_none());
    }
}
