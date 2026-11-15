// Responsibility: Entry point for the maio CLI runner, handling command dispatch and process exit.

mod args;
mod runner;

use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = args::Cli::parse();

    let log_level = match cli.verbose {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(log_level.into()))
        .compact()
        .init();

    runner::execute(cli)
}
