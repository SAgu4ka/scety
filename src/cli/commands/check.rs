use crate::config::get_services_config::get_all_configs;
use crate::config::settings::SERVICES_CONFIGS_PATH;
use tracing::{info, warn};

pub async fn check() -> Result<(), Box<dyn std::error::Error>> {
    info!("Checking configuration files...");
    let all_configs = get_all_configs(None);
    let raw_configs = crate::core::runtime::load_raw_service_configs(SERVICES_CONFIGS_PATH);

    #[cfg(feature = "l4")]
    let l4_configs = crate::l4::config::parse_l4_confs(raw_configs.clone()).await;
    #[cfg(feature = "static")]
    let static_configs = crate::lstatic::config::parse_static_configs(
        raw_configs
            .iter()
            .filter(|value| crate::lstatic::has_configs(std::slice::from_ref(value)))
            .flat_map(|value| {
                value
                    .get("configs")
                    .and_then(toml::Value::as_array)
                    .cloned()
                    .unwrap_or_else(|| vec![value.clone()])
            })
            .collect(),
    );

    let mut module_count = all_configs.len();
    #[cfg(feature = "l4")]
    {
        module_count += l4_configs.len();
    }
    #[cfg(feature = "static")]
    {
        module_count += static_configs.len();
    }

    if module_count == 0 {
        warn!(path=%SERVICES_CONFIGS_PATH, "No valid configuration files found");
        return Err("No valid configuration files found".into());
    }

    info!(
        "All {} enabled module configurations are valid",
        module_count
    );
    Ok(())
}
