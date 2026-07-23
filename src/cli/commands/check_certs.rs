use crate::config::get_scety_config::get_scety_config;
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::SERVICES_CONFIGS_PATH;
use crate::network::cert_check::check_all_configured_certs;
use tracing::{info, warn};

pub async fn check_certs() -> Result<(), Box<dyn std::error::Error>> {
    info!("Checking TLS certificates...");

    let trusted_ca_bundle = get_scety_config()?.and_then(|c| c.trusted_ca_bundle);

    let all_configs = get_all_configs(Some(false));
    if all_configs.is_empty() {
        warn!(path=%SERVICES_CONFIGS_PATH, "No valid configuration files found");
        Err("No valid configuration files found".into())
    } else if check_all_configured_certs(&all_configs, trusted_ca_bundle.as_deref()) {
        info!("All the certificates checked are in order");
        Ok(())
    } else {
        Err("Issues with TLS certificates detected, see warnings above".into())
    }
}
