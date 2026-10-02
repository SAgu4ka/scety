use regex::Regex;
// use std::fs;
use std::io::{Error, ErrorKind};
use std::sync::{Arc, LazyLock};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::cli::commands::install::install;
use crate::core::{Module, ScetyCore};

static SYSTEMD_RUN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"/(system\.slice|user\.slice/.*|app\.slice)/(run-[ru]\d+|[a-zA-Z0-9_-]+)\.(service|scope)",
    )
    .unwrap()
});

pub async fn run(
    force_install: bool,
    force_start: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    if force_install {
        install(false)?;
    }

    let is_systemd = match verify_systemd_run() {
        Ok(_) => true,
        Err(e) => {
            warn!(error = %e, "systemd check skipped");
            false
        }
    };

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
    // let cgroup_path = "/proc/self/cgroup";
    // let cgroup_content = fs::read_to_string(cgroup_path)
    //     .map_err(|_| "Failed to read /proc/self/cgroup. The environment might not be Linux")?;

    // if !SYSTEMD_RUN_REGEX.is_match(&cgroup_content) {
    //     return Err("Security violation: process started outside the systemd-run time unit");
    // }

    // let ppid = nix::unistd::getppid().as_raw();
    // if ppid == 1 {
    //     return Ok(());
    // }

    // let parent_comm = fs::read_to_string(format!("/proc/{}/comm", ppid))
    //     .map_err(|_| "Failed to verify the parent process")?;

    // if !parent_comm.trim().starts_with("systemd") {
    //     return Err("Security violation: the parent process is not systemd");
    // }

    // Ok(())
    if std::env::var_os("INVOCATION_ID").is_some() {
        return Ok(());
    }

    let cgroup_content = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|_| "Failed to read /proc/self/cgroup")?;

    if SYSTEMD_RUN_REGEX.is_match(&cgroup_content) {
        return Ok(());
    }

    Err("Process started outside of a systemd unit")
}

async fn start_core() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let modules = prepare_list_modules();

    let mut core = ScetyCore::new();
    core.init(modules)?;

    let core = Arc::new(Mutex::new(core));

    let core_for_reload = Arc::clone(&core);

    let reload_handle = tokio::spawn(async move {
        let mut sighup = signal(SignalKind::hangup()).expect("failed to setup SIGHUP handler");

        while sighup.recv().await.is_some() {
            info!("Reload signal received; reloading core..");
            let mut guard = core_for_reload.lock().await;
            if let Err(e) = guard.reload().await {
                error!(error = %e, "Error during core reload");
            }
        }
    });

    let core_for_run = Arc::clone(&core);
    let _run_handle = tokio::spawn(async move {
        let mut guard = core_for_run.lock().await;
        if let Err(e) = guard.run().await {
            error!(error = %e, "Error during the core's main execution loop");
        }
    });

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => info!("Received SIGINT (Ctrl+C)"),
        _ = sigterm.recv() => info!("Received SIGTERM"),
    }

    info!("Shutting down...");

    reload_handle.abort();

    let guard = core.lock().await;
    guard.shutdown();

    Ok(())
}

fn prepare_list_modules() -> Vec<Box<dyn Module>> {
    Vec::<Box<dyn Module>>::new() // This part will be implemented later
}
