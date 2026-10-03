use clap::Parser;
use tracing::warn;

use crate::cli::{
    Cli,
    Commands::{Install, Reload, Run, Status, Stop, Uninstall},
    commands::{
        install::install, reload::reload, run::run, status::status, stop::stop,
        uninstall::uninstall,
    },
    print_full_help,
};

mod cli;
mod core;

include!(concat!(env!("OUT_DIR"), "/generated_settings.rs"));
const PID_PATH: &str = "/run/scety/scety.sock";
pub type DynResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> DynResult<()> {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
            "%Y-%m-%d %H:%M:%S".to_string(),
        ))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOG_LEVEL")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Some(Run {
            force_install,
            force_start,
        }) => {
            run(*force_install, *force_start).await?;
        }
        Some(Install { force_reinstall }) => {
            install(*force_reinstall)?;
        }
        Some(Uninstall) => {
            uninstall()?;
        }
        Some(Reload) => {
            reload().await?;
        }
        Some(Status {
            follow,
            checks,
            interval,
        }) => {
            status(*follow, *checks, *interval)?;
        }
        Some(Stop { force }) => {
            stop(*force)?;
        }
        None => {
            print_full_help();
        }
        Some(other) => {
            warn!(command=?other, "This command not done yet");
        }
    }

    Ok(())
}
