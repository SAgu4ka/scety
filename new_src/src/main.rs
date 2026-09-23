use clap::Parser;
use tracing::warn;

use crate::cli::{
    Cli,
    Commands::{Reload, Status, Stop},
    commands::{reload::reload, status::status, stop::stop},
    print_full_help,
};

mod cli;
mod core;
mod settings;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        Some(Reload) => {
            reload()?;
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
