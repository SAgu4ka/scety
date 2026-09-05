use crate::cli::commands::install::install;
use crate::config::get_scety_config::{SCETY_CONFIG, ScetyConfig, get_scety_config};
use crate::config::get_services_config::get_all_configs;
use crate::config::settings::{EXPOSE_VERSION, SERVICES_CONFIGS_PATH};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

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
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ))
                .map_err(|_| "ScetyConfig was initialized twice!")?;
        }

        debug!("Start load configs...");
        let all_configs = get_all_configs(None);
        let raw_configs = crate::core::runtime::load_raw_service_configs(SERVICES_CONFIGS_PATH);

        debug!("Checking configured TLS certificates...");
        #[cfg(feature = "l7")]
        if !crate::l7::network::cert_check::check_all_configured_certs(
            &all_configs,
            crate::config::get_scety_config::scety_config()
                .trusted_ca_bundle
                .as_deref(),
        ) {
            warn!(
                "Issues with TLS certificates have been detected (see the note above). Scety will proceed with startup regardless—this is not a blocking check."
            );
        }

        info!(
            "Successfully loaded {} HTTP configs and {} raw module configs",
            all_configs.len(),
            raw_configs.len()
        );
        let token = CancellationToken::new();
        let runtime = crate::core::runtime::ProxyRuntime::start(
            all_configs,
            raw_configs,
            EXPOSE_VERSION,
            token,
        )
        .await;
        runtime.wait().await;

        Ok(())
    } else {
        install().await?;
        Ok(())
    }
}
