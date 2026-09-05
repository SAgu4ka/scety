use std::path::PathBuf;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

#[allow(dead_code)]
pub trait ProxyModule {
    const PROXY_TYPE: &'static str;

    fn accepts(config: &toml::Value) -> bool {
        config
            .get("proxy_type")
            .and_then(toml::Value::as_str)
            .is_some_and(|kind| kind.eq_ignore_ascii_case(Self::PROXY_TYPE))
    }
}

pub struct ProxyRuntime {
    tasks: JoinSet<()>,
    token: CancellationToken,
}

impl ProxyRuntime {
    pub async fn start(
        http_configs: Vec<crate::config::get_services_config::ClientConfig>,
        raw_configs: Vec<toml::Value>,
        expose_version: bool,
        token: CancellationToken,
    ) -> Self {
        #[allow(unused_mut)]
        let mut tasks = JoinSet::new();

        #[cfg(not(feature = "l7"))]
        let _ = (&http_configs, expose_version);
        #[cfg(not(feature = "l4"))]
        let _ = &raw_configs;

        #[cfg(feature = "l7")]
        if !http_configs.is_empty() {
            let module_token = token.child_token();
            tasks.spawn(async move {
                crate::l7::start(http_configs, expose_version, module_token).await;
            });
        }

        #[cfg(feature = "l4")]
        {
            let l4_configs = crate::l4::config::parse_l4_confs(raw_configs.clone()).await;
            if !l4_configs.is_empty() {
                info!(services = l4_configs.len(), "Starting L4 proxy module");
                let workers = crate::l4::engine::start_mio_listen_with_handles(l4_configs);
                let shutdown_token = token.child_token();
                tasks.spawn(async move {
                    shutdown_token.cancelled().await;
                    let (shutdowns, worker_handles): (Vec<_>, Vec<_>) = workers.into_iter().unzip();
                    for shutdown in shutdowns {
                        shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    let _ = tokio::task::spawn_blocking(move || {
                        for worker in worker_handles {
                            let _ = worker.join();
                        }
                    })
                    .await;
                });
            }
        }

        #[cfg(feature = "static")]
        if crate::lstatic::has_configs(&raw_configs) {
            let static_token = token.child_token();
            tasks.spawn(async move {
                crate::lstatic::start(raw_configs, static_token).await;
            });
        }

        #[cfg(not(any(feature = "l4", feature = "l7", feature = "static")))]
        {
            let _ = (http_configs, raw_configs, expose_version);
            warn!("Scety was built without proxy modules");
        }

        debug!("Proxy module runtime initialized");
        Self { tasks, token }
    }

    pub async fn wait(mut self) {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down proxy modules");
                self.token.cancel();
            }
            result = self.tasks.join_next() => {
                if let Some(Err(error)) = result {
                    warn!(%error, "Proxy module task exited unexpectedly");
                }
                self.token.cancel();
            }
        }
        while self.tasks.join_next().await.is_some() {}
    }
}

pub fn load_raw_service_configs(path: &str) -> Vec<toml::Value> {
    let mut configs = Vec::new();
    for entry in walkdir::WalkDir::new(PathBuf::from(path))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "toml"))
    {
        match std::fs::read_to_string(entry.path()) {
            Ok(content) => match toml::from_str(&content) {
                Ok(value) => configs.push(value),
                Err(error) => {
                    warn!(file = %entry.path().display(), %error, "Skipping invalid raw module configuration")
                }
            },
            Err(error) => {
                warn!(file = %entry.path().display(), %error, "Skipping unreadable raw module configuration")
            }
        }
    }
    configs
}
