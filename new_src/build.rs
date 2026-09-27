use serde::Deserialize;
use std::{env, fs, path::Path};

#[derive(Deserialize)]
struct Settings {
    #[serde(default)]
    custom_modules: Vec<String>,
    #[serde(default = "default_main_path")]
    main_path: String,
    #[serde(default = "default_user")]
    scety_user: String,
}

fn default_main_path() -> String {
    "/var/lib/scety".to_string()
}
fn default_user() -> String {
    "scety".to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=settings.toml");

    let out_dir = env::var("OUT_DIR")?;
    let dest_dir = Path::new(&out_dir).join("custom_modules");
    fs::create_dir_all(&dest_dir)?;

    let settings_file = fs::read_to_string("settings.toml").unwrap_or_default();
    let settings: Settings = toml::from_str(&settings_file).unwrap_or(Settings {
        custom_modules: vec![],
        main_path: default_main_path(),
        scety_user: default_user(),
    });

    let client = reqwest::blocking::Client::new();

    let mut all_module_paths = Vec::new();

    for (i, raw_url) in settings.custom_modules.iter().enumerate() {
        let clean_url = raw_url.replace('\\', "/");

        let download_url = normalize_github_url(&clean_url);

        let module_filename = format!("module_{}.rs", i);
        let target_path = dest_dir.join(&module_filename);

        let response = client.get(&download_url).send()?;
        if response.status().is_success() {
            let content = response.text()?;
            fs::write(&target_path, content)?;
            all_module_paths.push(target_path.to_string_lossy().to_string());
        } else {
            println!("cargo:warning=Failed to download module from {}", clean_url);
        }
    }

    let generated_code = format!(
        r#"pub static MODULE_PATHS: &[&str] = &{:?};
pub static MAIN_PATH: &'static str = {:?};
pub static SCETY_USER: &'static str = {:?};
    "#,
        all_module_paths, settings.main_path, settings.scety_user,
    );

    let generated_file_path = Path::new(&out_dir).join("generated_settings.rs");
    fs::write(generated_file_path, generated_code)?;

    Ok(())
}

fn normalize_github_url(url: &str) -> String {
    if url.contains("github.com") && !url.contains("raw.githubusercontent.com") {
        url.replace("github.com", "raw.githubusercontent.com")
            .replace("/blob/", "/")
    } else {
        url.to_string()
    }
}
