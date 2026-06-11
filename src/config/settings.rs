// THIS FILE ONLY FOR PARSE FROM 'settings.toml'

use static_toml::static_toml;

static_toml! {
    static CONFIG = include_toml!("src/setting.toml");
}

pub const SERVISES_CONFIGS_PATH: &str = CONFIG.paths.servises_configs_path;
pub const EXPOSE_VERSION: bool = CONFIG.main.expose_version;