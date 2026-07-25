use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/setting.toml");

    let toml_content =
        fs::read_to_string("src/setting.toml").expect("Failed to read src/setting.toml");

    let parsed_toml: toml::Value =
        toml::from_str(&toml_content).expect("TOML validation error in src/setting.toml");

    let relative_code_table_path = parsed_toml
        .get("paths")
        .and_then(|v| v.get("http_code_table"))
        .and_then(|v| v.as_str())
        .expect("`paths.http_code_table` is missing from `src/setting.toml`.");

    let code_table_path = Path::new(relative_code_table_path);

    if code_table_path.exists() {
        println!("cargo:rerun-if-changed={}", code_table_path.display());
    }

    let code_table_content = fs::read_to_string(code_table_path).unwrap_or_else(|_| {
        panic!(
            "Could not find the code table file at the path: {:?}",
            code_table_path
        )
    });

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_code_table.rs");

    let generated_code = format!(
        "pub const HTTP_CODE_TABLE: &str = {:?};",
        code_table_content
    );

    fs::write(&dest_path, generated_code).expect("Failed to write the generated file");
}
