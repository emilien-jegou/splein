// crates/splein/src/main.rs

use clap::Parser;
use std::error::Error;
use std::process::ExitCode;

mod app;
mod cli;
mod commons;
mod domain;
mod platform;
mod render;

use cli::{Args, Commands};
use platform::ipc::IpcClient;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = Args::parse();

    // Enable console logging by default so daemon logs are always visible
    let _trace_guard = commons::tracing::Tracer::builder()
        .flamegraph_enable(args.common.flamegraph_enable)
        .flamegraph_save_file(args.common.flamegraph_save_file.clone())
        .log_enable(true)
        .log_save_path(args.common.log_save_path.clone())
        .log_console(true)
        .build()
        .setup()?;

    match args.command.unwrap_or(Commands::Daemon) {
        Commands::Daemon => {
            tracing::info!("Starting Presentify Wayland Daemon...");
            platform::wayland::run_overlay_daemon()?;
        }
        Commands::Toggle => {
            tracing::info!("Sending 'toggle' command to daemon...");
            IpcClient::send_command("toggle")?;
            tracing::info!("'toggle' command delivered successfully");
        }
        Commands::Clear => {
            tracing::info!("Sending 'clear' command to daemon...");
            IpcClient::send_command("clear")?;
        }
        Commands::Undo => {
            tracing::info!("Sending 'undo' command to daemon...");
            IpcClient::send_command("undo")?;
        }
    }

    Ok(ExitCode::SUCCESS)
}
