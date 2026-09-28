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
        Commands::Toggle { screen } => {
            // If no screen is provided, detect the monitor that currently has the cursor
            let target_screen = screen.or_else(detect_focused_monitor);
            let cmd = match target_screen {
                Some(ref name) => format!("toggle {}", name),
                None => "toggle".to_string(),
            };

            tracing::info!("Sending '{}' command to daemon...", cmd);
            IpcClient::send_command(&cmd)?;
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

/// Queries Hyprland or Sway IPC to find the monitor currently holding focus / cursor
fn detect_focused_monitor() -> Option<String> {
    // 1. Try Hyprland
    if let Ok(output) = std::process::Command::new("hyprctl").arg("monitors").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut current_monitor = None;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("Monitor ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2 {
                        current_monitor = Some(parts[1].to_string());
                    }
                } else if trimmed.starts_with("focused: yes") {
                    if let Some(mon) = current_monitor {
                        return Some(mon);
                    }
                }
            }
        }
    }

    // 2. Try Sway
    if let Ok(output) = std::process::Command::new("swaymsg").args(["-t", "get_outputs", "-r"]).output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Search for "focused":true
            if let Some(focused_idx) = text.find("\"focused\":true") {
                let before = &text[..focused_idx];
                if let Some(name_idx) = before.rfind("\"name\":\"") {
                    let start = name_idx + 8;
                    if let Some(end) = before[start..].find('"') {
                        return Some(before[start..start + end].to_string());
                    }
                }
            }
        }
    }

    None
}
