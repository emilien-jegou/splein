// Application entry point initializing telemetry, CLI options, and daemon execution.

use clap::Parser;
use std::error::Error;
use std::process::ExitCode;

mod app;
mod cli;
mod commons;
mod domain;
mod platform;
mod render;
pub mod ui;

use cli::{Args, Commands};
use platform::ipc::IpcClient;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = Args::parse();

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
            tracing::info!("Starting Splein Daemon...");
            platform::wayland::daemon::run_overlay_daemon()?;
        }
        Commands::Toggle { screen } => {
            let cmd = screen.map_or("toggle".to_string(), |s| format!("toggle {}", s));
            if !IpcClient::try_send_command(&cmd)? {
                tracing::info!("No daemon found; starting a standalone overlay");
                platform::wayland::daemon::run_standalone_overlay()?;
            }
        }
        Commands::Clear => {
            IpcClient::send_command("clear")?;
        }
        Commands::Undo => {
            IpcClient::send_command("undo")?;
        }
    }

    Ok(ExitCode::SUCCESS)
}
