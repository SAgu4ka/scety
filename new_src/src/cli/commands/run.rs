use regex::Regex;
use std::fs;
use std::io::{Error, ErrorKind};
use std::sync::LazyLock;
use tracing::error;

use crate::cli::commands::install::install;

static SYSTEMD_RUN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/(system\.slice|user\.slice/.*|app\.slice)/(run-[ru]\d+|[a-zA-Z0-9_-]+)\.(service|scope)").unwrap()
});

pub async fn run(force_install: bool, force_start: bool) -> Result<(), Box<dyn std::error::Error>> {
    if force_install {
        install(false)?;
    }

    let is_systemd = verify_systemd_run().is_ok();

    if force_start || is_systemd {
        start_core().await?;
    } else {
        let err_msg = "For security reasons, you cannot simply launch scety as your own user. Please use `systemd-run` / `systemctl start scety` (or `--force-start` at your own risk)";
        error!("{}", err_msg);
        return Err(Box::new(Error::new(ErrorKind::PermissionDenied, err_msg)));
    }
    Ok(())
}

fn verify_systemd_run() -> Result<(), &'static str> {
    let cgroup_path = "/proc/self/cgroup";
    let cgroup_content = fs::read_to_string(cgroup_path)
        .map_err(|_| "Failed to read /proc/self/cgroup. The environment might not be Linux")?;

    if !SYSTEMD_RUN_REGEX.is_match(&cgroup_content) {
        return Err("Security violation: process started outside the systemd-run time unit");
    }

    let ppid = nix::unistd::getppid().as_raw();
    if ppid == 1 {
        return Ok(());
    }

    let parent_comm = fs::read_to_string(format!("/proc/{}/comm", ppid))
    .map_err(|_| "Failed to verify the parent process")?;

    if !parent_comm.trim().starts_with("systemd") {
        return Err("Security violation: the parent process is not systemd");
    }

    Ok(())
}

async fn start_core() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
