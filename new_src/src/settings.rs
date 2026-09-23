/*!
This file will be converted into a more convenient TOML file, with parsing handled in `build.rs`
*/

pub const MAIN_SCETY_PATH: &str = "/etc/scety";

// SCETY SERVICES consts
pub const MAX_CONFIG_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB
pub const ALLOW_LINKS_IN_CONFIGS_DIR: bool = true;
pub const ALLOW_FOLLOW_NONBASE_SYMLINK_DIR: bool = false;

pub fn _check() {
    if ALLOW_FOLLOW_NONBASE_SYMLINK_DIR && !ALLOW_LINKS_IN_CONFIGS_DIR {
        panic!()
    }
}
