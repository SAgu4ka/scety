use clap::Parser;
use cli::args::{Cli, Commands};
use tracing::{error};
use crate::cli::commands::{check::check, install::install, reload::reload, run::run, status::status, stop::stop, uninstall::uninstall };

mod http;
mod core;
mod config;
mod network;
mod cli;

async fn run_command<F, Fut>(f: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    if let Err(e) = f().await {
        error!("{}", e);
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>{
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S".to_string()))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOG_LEVEL")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let cli = Cli::parse();
    
    match cli.command {
        Commands::Run => run_command(run).await,
        Commands::Stop => run_command(stop).await,
        Commands::Reload => run_command(reload).await,
        Commands::Status => run_command(status).await,
        Commands::Check => run_command(check).await,
        Commands::Uninstall => run_command(uninstall).await,
        Commands::Install => run_command(install).await,
    }

    Ok(()) 
}