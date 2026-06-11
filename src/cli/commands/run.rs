use crate::cli::commands::install::install;
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::{EXPOSE_VERSION, SERVICES_CONFIGS_PATH};
use crate::network::fallback_server::start_fallback_server;
use crate::network::global_router::start_listen;
use tracing::{debug, info};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let is_systemd = std::env::var("INVOCATION_ID").is_ok();

    if is_systemd {
        info!("Start scety...");
        debug!(config_path=%SERVICES_CONFIGS_PATH, expose_version=%EXPOSE_VERSION, "Starting arguments");
        debug!("Start load configs...");
        let all_configs = get_all_configs();
        if all_configs.is_empty() {
            start_fallback_server().await?;
        } else {
            info!("Successfully loaded {} configs", all_configs.len());
            info!("Start listeners...");
            start_listen(all_configs, EXPOSE_VERSION);
            tokio::signal::ctrl_c().await?;
        }
        Ok(())
    } else {
        install().await?;
        Ok(())
    }
}
