use crate::cli::commands::install::install;
use crate::config::get_scety_config::{SCETY_CONFIG, ScetyConfig, get_scety_config};
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::{EXPOSE_VERSION, SERVICES_CONFIGS_PATH};
use crate::network::fallback_server::start_fallback_server;
use crate::network::global_router::start_listen;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let is_systemd = std::env::var("INVOCATION_ID").is_ok();

    if is_systemd {
        info!("Start scety...");
        debug!(config_path=%SERVICES_CONFIGS_PATH, expose_version=%EXPOSE_VERSION, "Starting arguments");

        debug!("Loading main ScetyConfig...");
        if let Some(loaded_config) = get_scety_config()? {
            SCETY_CONFIG
                .set(loaded_config)
                .map_err(|_| "ScetyConfig was initialized twice!")?;
            debug!("Main ScetyConfig successfully initialized");
        } else {
            warn!("ScetyConfig file exists but parsed as empty. Using defaults.");
            SCETY_CONFIG
                .set(ScetyConfig::new(
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    std::collections::HashMap::new(),
                    std::collections::HashMap::new(),
                ))
                .map_err(|_| "ScetyConfig was initialized twice!")?;
        }

        debug!("Start load configs...");
        let all_configs = get_all_configs();

        debug!("Checking configured TLS certificates...");
        if !crate::network::cert_check::check_all_configured_certs(
            &all_configs,
            crate::config::get_scety_config::scety_config()
                .trusted_ca_bundle
                .as_deref(),
        ) {
            warn!(
                "Обнаружены проблемы с TLS-сертификатами (см. предупреждения выше). scety всё равно продолжит запуск — это не блокирующая проверка."
            );
        }

        if all_configs.is_empty() {
            start_fallback_server().await?;
        } else {
            info!("Successfully loaded {} configs", all_configs.len());
            info!("Start listeners...");
            let token = CancellationToken::new();
            let mut listeners = start_listen(all_configs, EXPOSE_VERSION, token.clone());
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutting down...");
                    token.cancel();
                }
                res = listeners.join_next() => {
                    error!("Listener unexpectedly died: {:?}", res);
                    std::process::exit(1);
                }
            }
            listeners.join_all().await;
        }

        Ok(())
    } else {
        install().await?;
        Ok(())
    }
}
