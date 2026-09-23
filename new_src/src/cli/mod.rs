pub mod commands;

use clap::builder::styling::{AnsiColor, Effects, Reset, Styles};
use clap::{CommandFactory, Parser, Subcommand};

fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Parser)]
#[command(
    name = "scety",
    about = "Just a reverse proxy",
    version = env!("CARGO_PKG_VERSION"),
    styles = cli_styles(),
    arg_required_else_help = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Starts the service (intended for systemd execution only)
    Run {
        /// If executed by a user with this flag, installs the service if it is not installed yet
        #[arg(short, long)]
        force_install: bool,

        /// Bypasses the check for whether the proxy is installed as a systemd service and starts it immediately in the current user context
        #[arg(short, long)]
        force_start: bool,
    },
    /// Stops the service
    Stop {
        /// Forcefully and immediately stops the service
        #[arg(short, long)]
        force: bool,
    },
    /// Reloads the service configuration
    Reload,
    /// Checks the service status
    Status {
        /// Continuously tracks the status until interrupted by the user (incompatible with 'checks' flag)
        #[arg(
            short = 'f',
            long = "follow",
            conflicts_with_all = ["checks"]
        )]
        follow: bool,

        /// Number of status checks to perform
        #[arg(short = 'c', long = "checks")]
        checks: Option<u16>,

        /// Interval between checks in seconds
        #[arg(short = 'i', long = "interval")]
        interval: Option<u32>,
    },
    /// Validates the entire configuration file
    Check {
        /// Validates only the specified configuration files
        #[arg(short, long)]
        config: Vec<String>,
    },
    /// Validates the TLS certificate configuration
    CheckCerts {
        /// Validates only the specified configuration files
        #[arg(short, long)]
        config: Vec<String>,
    },
    /// Uninstalls the service from the system
    Uninstall,
    /// Installs the service into the system
    Install {
        /// Reinstalls the service from scratch if it is already installed
        #[arg(short, long)]
        force_reinstall: bool,
    },
}

pub fn print_full_help() {
    let mut cmd = Cli::command();

    cmd.print_help().unwrap();
    println!("\n");

    let header_style = format!("{}{}", AnsiColor::Green.render_fg(), Effects::BOLD.render());
    let literal_style = format!("{}{}", AnsiColor::Cyan.render_fg(), Effects::BOLD.render());
    let reset = Reset.render();

    let mut subcommands_with_args = Vec::new();
    for sub in cmd.get_subcommands_mut() {
        if sub.get_name() == "help" {
            continue;
        }

        let has_custom_args = sub
            .get_arguments()
            .any(|arg| arg.get_id() != "help" && arg.get_id() != "version");

        if has_custom_args {
            subcommands_with_args.push(sub);
        }
    }

    if !subcommands_with_args.is_empty() {
        println!("{header_style}Subcommand Flags:{reset}");

        for sub in subcommands_with_args {
            println!("  {literal_style}{}{reset}:", sub.get_name());

            for arg in sub.get_arguments() {
                if arg.get_id() == "help" || arg.get_id() == "version" {
                    continue;
                }

                let short = arg.get_short().map(|s| format!("-{s}")).unwrap_or_default();
                let long = arg.get_long().map(|l| format!("--{l}")).unwrap_or_default();

                let flags = match (!short.is_empty(), !long.is_empty()) {
                    (true, true) => format!("{short}, {long}"),
                    (true, false) => short,
                    (false, true) => long,
                    _ => arg.get_id().to_string(),
                };

                let help = arg.get_help().unwrap_or_default();

                println!("    {literal_style}{flags:<24}{reset} {help}");
            }
            println!();
        }
    }
}
