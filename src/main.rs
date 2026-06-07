use clap::Parser;
use cli::args::{Cli, Commands};

use crate::cli::commands::run::run;

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
        Commands::Run => { run().await?; }
        Commands::Stop => {  }
        Commands::Reload => {  }
        Commands::Status => {  }
        Commands::Check => {  }
        Commands::Uninstall => {  }
    }

    Ok(()) 
}