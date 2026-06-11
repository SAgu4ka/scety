use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "scety", about = "Just reverse proxy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Run,
    Stop,
    Reload,
    Status,
    Check,
    Uninstall,
    Install,
}
