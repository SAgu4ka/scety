use serde::Deserialize;
use serde_inline_default::serde_inline_default;
use std::{env, fs, path::Path};

#[serde_inline_default]
#[derive(Deserialize, Default)]
struct ConfigsSettings {
    #[serde_inline_default(10 * 1024 * 1024)] // 10 MB
    max_config_size_bytes: u64,

    #[serde_inline_default(true)]
    allow_links_in_configs_dir: bool,

    #[serde_inline_default(false)]
    allow_follow_nonbase_symlink_dir: bool,
}

#[serde_inline_default]
#[derive(Deserialize, Default)]
struct Settings {
    // #[serde(default)]
    // custom_modules: Vec<String>,
    #[serde_inline_default("/var/lib/scety".to_string())]
    main_path: String,

    #[serde_inline_default("/etc/scety".to_string())]
    main_scety_path: String,

    #[serde_inline_default("scety".to_string())]
    scety_user: String,

    #[serde(default)]
    configs_settings: ConfigsSettings,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=settings.toml");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR")?;
    let dest_dir = Path::new(&out_dir).join("custom_modules");
    fs::create_dir_all(&dest_dir)?;

    let settings_file = fs::read_to_string("settings.toml").unwrap_or_default();
    let settings: Settings = toml::from_str(&settings_file).unwrap_or_else(|_| Settings::default());

    if settings.configs_settings.allow_follow_nonbase_symlink_dir
        && !settings.configs_settings.allow_links_in_configs_dir
    {
        panic!(
            "'allow_follow_nonbase_symlink_dir' cannot be enabled when 'allow_links_in_configs_dir' is disabled."
        )
    }

    // let client = reqwest::blocking::Client::new();

    // let mut all_module_paths = Vec::new();

    // for (i, raw_url) in settings.custom_modules.iter().enumerate() {
    //     let clean_url = raw_url.replace('\\', "/");

    //     let download_url = normalize_github_url(&clean_url);

    //     let module_filename = format!("module_{}.rs", i);
    //     let target_path = dest_dir.join(&module_filename);

    //     match client.get(&download_url).send() {
    //         Ok(response) if response.status().is_success() => {
    //             if let Ok(content) = response.text() {
    //                 fs::write(&target_path, content)?;
    //                 all_module_paths.push(target_path.to_string_lossy().to_string());
    //             }
    //         }
    //         _ => {
    //             println!("cargo:warning=Failed to download module from {}", clean_url);
    //         }
    //     }
    // }

    let generated_code = format!(
        // pub static MODULE_PATHS: &[&str] = &{:?};
        r#"pub static MAIN_PATH: &str = {:?};
pub static SCETY_USER: &str = {:?};
pub const MAX_CONFIG_SIZE_BYTES: u64 = {:?};
pub const ALLOW_LINKS_IN_CONFIGS_DIR: bool = {:?};
pub const ALLOW_FOLLOW_NONBASE_SYMLINK_DIR: bool = {:?};
pub const MAIN_SCETY_PATH: &str = {:?};
    "#,
        // all_module_paths,
        settings.main_path,
        settings.scety_user,
        settings.configs_settings.max_config_size_bytes,
        settings.configs_settings.allow_links_in_configs_dir,
        settings.configs_settings.allow_follow_nonbase_symlink_dir,
        settings.main_scety_path
    );

    let generated_file_path = Path::new(&out_dir).join("generated_settings.rs");
    fs::write(generated_file_path, generated_code)?;

    Ok(())
}

// fn normalize_github_url(url: &str) -> String {
//     if url.contains("github.com") && !url.contains("raw.githubusercontent.com") {
//         url.replace("github.com", "raw.githubusercontent.com")
//             .replace("/blob/", "/")
//     } else {
//         url.to_string()
//     }
// }
