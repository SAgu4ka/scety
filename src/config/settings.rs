// THIS FILE ONLY FOR PARSE FROM 'settings.toml'

use const_format::formatcp;
use static_toml::static_toml;

static_toml! {
    static CONFIG = include_toml!("src/setting.toml");
}

pub const EXPOSE_VERSION: bool = CONFIG.main.expose_version;
pub const MAIN_SCETY_PATH: &str = CONFIG.paths.main_scety_path;

pub const SERVICES_CONFIGS_PATH: &str = formatcp!("{}/services", MAIN_SCETY_PATH);
pub const SCETY_CONFIG_PATH: &str = formatcp!("{}/scety.toml", MAIN_SCETY_PATH);
