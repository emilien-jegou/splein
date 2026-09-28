// crates/splein/src/cli.rs
use clap::{Args as ClapArgs, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "splein", author, version, about = "Vector Screen Annotation Overlay for Wayland")]
pub struct Args {
    #[command(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(ClapArgs, Debug, Clone, Default)]
pub struct CommonArgs {
    #[arg(long, default_value_t = false)]
    pub flamegraph_enable: bool,

    #[arg(long)]
    pub flamegraph_save_file: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub log_enable: bool,

    #[arg(long)]
    pub log_save_path: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub log_console: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the background Wayland overlay daemon
    Daemon,
    /// Instant 0ms toggle overlay visibility / click-through
    Toggle,
    /// Clear all drawings
    Clear,
    /// Undo the last stroke
    Undo,
}
