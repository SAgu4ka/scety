use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Duration;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

use crate::cli::commands::install::install;
use crate::core::{Module, ScetyCore};
use crate::{DynResult, PID_PATH};

struct PidFileGuard {
    _file: File,
    path: PathBuf,
}

impl PidFileGuard {
    fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;

        let meta = file.metadata()?;
        if !meta.file_type().is_file() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "PID path is not a regular file",
            ));
        }

        let lock_res = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

        if lock_res != 0 {
            let err = Error::last_os_error();
            if err.kind() == ErrorKind::WouldBlock || err.raw_os_error() == Some(libc::EWOULDBLOCK)
            {
                return Err(Error::new(
                    ErrorKind::AlreadyExists,
                    "Another instance of scety is already running!",
                ));
            }
            return Err(err);
        }

        file.set_len(0)?;
        write!(file, "{}", process::id())?;

        Ok(Self { _file: file, path })
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self._file.as_raw_fd(), libc::LOCK_UN);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn run(force_install: bool, force_start: bool) -> DynResult<()> {
    if !nix::unistd::Uid::effective().is_root() && (force_install || force_start) {
        error!("Run as root or with sudo");
        return Err(Box::new(Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Run as root or with sudo",
        )));
    }

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

pub fn verify_systemd_run() -> Result<(), &'static str> {
    let content = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|_| "Failed to read /proc/self/cgroup")?;

    for line in content.lines() {
        let path = match line.splitn(3, ':').nth(2) {
            Some(p) => p,
            None => continue,
        };

        if path.starts_with("/system.slice/") || path == "/init.scope" {
            return Ok(());
        }
    }

    Err("Process started outside of a system-level systemd unit")
}

async fn start_core() -> DynResult<()> {
    let _pid_guard = match PidFileGuard::new(PID_PATH) {
        Ok(guard) => guard,
        Err(e) => {
            error!(error = %e, "Failed to start: process lock conflict");
            return Err(e.into());
        }
    };

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigusr1 = signal(SignalKind::user_defined1())?;

    let modules = prepare_list_modules();
    let mut core = ScetyCore::new();
    core.init(modules)?;

    let mut join_set = core.run().await?;

    info!("Scety core started successfully");

    loop {
        tokio::select! {
            _ = sigusr1.recv() => {
                info!("Reload signal received; reloading core..");
                if let Err(e) = core.reload().await {
                    error!(error = %e, "Error during core reload");
                }
            }
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C)");
                break;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM");
                break;
            }
            res = join_set.join_next() => {
                match res {
                    Some(Ok(Ok(()))) => {
                        warn!("One of the modules exited gracefully");
                    }
                    Some(Ok(Err(e))) => {
                        error!(error = %e, "A module failed with error");
                    }
                    Some(Err(e)) if e.is_cancelled() => {
                        info!("Module task was cancelled");
                    }
                    Some(Err(e)) => {
                        error!(error = %e, "A module task panicked");
                    }
                    None => {
                        warn!("All modules have exited; stopping core");
                        break;
                    }
                }
            }
        }
    }

    info!("Shutting down Scety core...");

    core.shutdown();

    let shutdown_deadline = Duration::from_secs(10);
    match tokio::time::timeout(shutdown_deadline, async {
        while let Some(res) = join_set.join_next().await {
            if let Err(e) = res {
                error!(error = %e, "Error waiting for module task during shutdown");
            }
        }
    })
    .await
    {
        Ok(()) => info!("All modules shut down cleanly"),
        Err(_) => {
            warn!("Shutdown timeout; aborting remaining tasks");
            join_set.abort_all();
            while join_set.join_next().await.is_some() {}
        }
    }

    info!("Scety core stopped cleanly.");

    Ok(())
}

fn prepare_list_modules() -> Vec<Box<dyn Module>> {
    Vec::<Box<dyn Module>>::new() // This part will be implemented later
}
