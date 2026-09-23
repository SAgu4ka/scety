use crate::settings::{
    ALLOW_FOLLOW_NONBASE_SYMLINK_DIR, ALLOW_LINKS_IN_CONFIGS_DIR, MAIN_SCETY_PATH,
    MAX_CONFIG_SIZE_BYTES,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Error, ErrorKind, Read, Result},
    path::{Path, PathBuf},
};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

#[derive(Deserialize)]
struct ConfigModeHeader {
    mode: Option<String>,
}

pub fn get_all_configs() -> Result<HashMap<String, Vec<PathBuf>>> {
    debug!("Starting search for configuration files...");
    let services_path = Path::new(MAIN_SCETY_PATH).join("services");

    let canonical_base = services_path.canonicalize().map_err(|err| {
        error!(path = ?services_path, error = %err, "Services directory is invalid or unreachable");
        Error::new(
            ErrorKind::NotFound,
            "Services path does not exist or is invalid",
        )
    })?;

    let mut configs: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for entry in WalkDir::new(&canonical_base).follow_links(ALLOW_FOLLOW_NONBASE_SYMLINK_DIR) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!(error = %err, "Failed to read directory entry during traversal");
                continue;
            }
        };

        if entry.file_type().is_dir() {
            debug!(path = ?entry.path(), "Entry is a directory, skipping");
            continue;
        }

        let is_symlink = entry.path_is_symlink();

        if is_symlink && !ALLOW_LINKS_IN_CONFIGS_DIR {
            debug!(path = ?entry.path(), "Entry is a symbolic link, build settings prevent it, skipping");
            continue;
        }

        let path = entry.path();

        let canonical_path = match path.canonicalize() {
            Ok(cp) => cp,
            Err(err) => {
                warn!(file = ?path, error = %err, "Failed to resolve canonical path");
                continue;
            }
        };

        let is_outside_base = !canonical_path.starts_with(&canonical_base);
        if is_outside_base {
            let allowed_by_file_symlink = is_symlink && ALLOW_LINKS_IN_CONFIGS_DIR;
            let allowed_by_dir_symlink = ALLOW_FOLLOW_NONBASE_SYMLINK_DIR;

            if !allowed_by_file_symlink && !allowed_by_dir_symlink {
                warn!(
                    file = ?canonical_path,
                    "File is outside the services directory and current link settings forbid it; skipping"
                );
                continue;
            }
        }

        let is_toml = canonical_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"));

        if !is_toml {
            debug!(
                path = ?canonical_path,
                extension = ?canonical_path.extension(),
                "This entry is not a 'toml' file, skipping"
            );
            continue;
        }

        if !seen_paths.insert(canonical_path.clone()) {
            debug!(path = ?canonical_path, "Found duplicate path, skipping");
            continue;
        }

        let file = match File::open(&canonical_path) {
            Ok(f) => f,
            Err(err) => {
                warn!(file = ?canonical_path, error = %err, "Failed to open config file");
                continue;
            }
        };

        let metadata = match file.metadata() {
            Ok(meta) => meta,
            Err(err) => {
                warn!(file = ?canonical_path, error = %err, "Failed to read file metadata");
                continue;
            }
        };

        if !metadata.is_file() {
            debug!(path = ?canonical_path, "Entry is not a regular file, skipping");
            continue;
        }

        let meta_len = metadata.len();
        if meta_len > MAX_CONFIG_SIZE_BYTES {
            warn!(
                file = ?canonical_path,
                size = meta_len,
                limit = MAX_CONFIG_SIZE_BYTES,
                "Config file exceeds maximum allowed size"
            );
            continue;
        }

        let mut content = String::with_capacity(meta_len as usize);
        let read_result = file
            .take(MAX_CONFIG_SIZE_BYTES + 1)
            .read_to_string(&mut content);

        if let Err(err) = read_result {
            warn!(file = ?canonical_path, error = %err, "Failed to read config file content");
            continue;
        }

        if content.len() as u64 > MAX_CONFIG_SIZE_BYTES {
            error!(
                path = ?canonical_path,
                "The file size exceeded limit during reading (TOCTOU swap detected)"
            );
            continue;
        }

        let config_header: ConfigModeHeader = match toml::from_str(&content) {
            Ok(h) => h,
            Err(err) => {
                warn!(file = ?canonical_path, error = %err, "Failed to parse TOML syntax");
                continue;
            }
        };

        if let Some(mode) = config_header.mode {
            info!(file = ?canonical_path, mode = %mode, "Found configuration file");
            configs.entry(mode).or_default().push(canonical_path);
        } else {
            warn!(file = ?canonical_path, "Missing 'mode' field in configuration file");
        }
    }

    Ok(configs)
}
