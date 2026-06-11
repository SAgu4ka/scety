use crate::config::get_services_config::get_all_configs;
use crate::config::settings::SERVICES_CONFIGS_PATH;
use tracing::{info, warn};

pub async fn check() -> Result<(), Box<dyn std::error::Error>> {
    info!("Checking configuration files...");
    let all_configs = get_all_configs();

    if all_configs.is_empty() {
        warn!(path=%SERVICES_CONFIGS_PATH, "No valid configuration files found");
        return Err("No valid configuration files found".into());
    }

    info!("All {} configuration files are valid", all_configs.len());
    Ok(())
}
