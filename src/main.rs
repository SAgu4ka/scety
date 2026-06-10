use clap::Parser;
use cli::args::{Cli, Commands};
use tracing::{error};
use crate::cli::commands::{run::run, stop::stop, status::status };

mod http;
mod core;
mod config;
mod network;
mod cli;

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
        Commands::Run => { 
            if let Err(e) = run().await{
                error!("{}", e);
                std::process::exit(1);
            }
        }
        Commands::Stop => { 
            if let Err(e) = stop().await {
                error!("{}", e);
                std::process::exit(1);
            }
        }   
        Commands::Reload => { status().await? }
        Commands::Status => {  }
        Commands::Check => {  }
        Commands::Uninstall => {  }
    }

    Ok(()) 
}