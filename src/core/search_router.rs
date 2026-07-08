use crate::config::get_services_config::ClientConfig;
use crate::core::host_router::HostRouter;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

pub struct SearchRouter {
    router: HostRouter,
    configs: Vec<ClientConfig>,
    cache: Mutex<LruCache<String, Option<usize>>>,
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
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(128).expect("cache capacity must be non-zero"),
            )),
            cache_capacity: 128,
        }
    }

    pub fn find(&self, host: &str) -> Option<&ClientConfig> {
        self.find_with_cache(host)
    }

    fn find_with_cache(&self, host: &str) -> Option<&ClientConfig> {
        let mut cache = self.cache.lock().expect("cache lock poisoned");
        if let Some(cached_index) = cache.get(host).copied() {
            return cached_index.and_then(|index| self.configs.get(index));
        }

        let resolved_index = self.router.matches(host);
        cache.put(host.to_string(), resolved_index);

        resolved_index.and_then(|index| self.configs.get(index))
    }
}

impl SearchRouter {
    #[allow(dead_code)]
    pub fn with_cache_capacity(mut self, capacity: usize) -> Self {
        self.cache_capacity = capacity;
        self.cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(capacity).expect("cache capacity must be non-zero"),
        ));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::get_services_config::{ClientConfig, UpstreamConfig};
    use std::collections::HashMap;
    use std::sync::Arc;

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

    #[tokio::test]
    async fn concurrent_find_calls_do_not_panic() {
        let router = Arc::new(SearchRouter::new(vec![
            make_config("example.com", 3000),
            make_config("api.example.com", 4000),
            make_config("foo.example.com", 5000),
        ]));

        let mut handles = Vec::new();
        for _ in 0..16 {
            let router = Arc::clone(&router);
            handles.push(tokio::spawn(async move {
                for host in [
                    "example.com",
                    "api.example.com",
                    "foo.example.com",
                    "missing.example.com",
                ] {
                    let cfg = router.find(host);
                    if host == "missing.example.com" {
                        assert!(cfg.is_none());
                    } else {
                        assert!(cfg.is_some());
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.expect("task panicked");
        }
    }

    #[test]
    #[ignore]
    fn benchmark_find_with_and_without_cache() {
        let patterns: Vec<ClientConfig> = (0..300)
            .map(|i| make_config(&format!("*.service{}.example.com", i), 3000 + i as u16))
            .collect();

        let router = SearchRouter::new(patterns.clone());
        let host = "api.service150.example.com";

        let start_no_cache = std::time::Instant::now();
        for _ in 0..10_000 {
            let _ = router.router.matches(host);
        }
        let elapsed_no_cache = start_no_cache.elapsed();

        let start_cached = std::time::Instant::now();
        for _ in 0..10_000 {
            let _ = router.find(host);
        }
        let elapsed_cached = start_cached.elapsed();

        eprintln!(
            "no cache: {:?}, cached: {:?}",
            elapsed_no_cache, elapsed_cached
        );
    }
}
