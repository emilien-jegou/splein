// Responsibility: Declarative command-line interface definition and argument parsing via clap.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "maio", version, about = "Asset compilation engine for .maiodb")]
pub struct Cli {
    #[arg(long, global = true, help = "Path to workspace root (defaults to auto-discovery)")]
    pub root: Option<PathBuf>,

    #[arg(short, long, global = true, action = clap::ArgAction::Count, help = "Verbosity level")]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Compile all pending tasks in the .maiodb manifest")]
    Build(BuildArgs),
    #[command(about = "Display journal summary and health status")]
    Status,
    #[command(name = "ls", alias = "list", about = "List all registered tasks in a tabular view")]
    List(ListArgs),
    #[command(about = "Inspect a specific asset by its AssetId")]
    Inspect(InspectArgs),
    #[command(about = "Prune orphaned baked artifacts from target/assets")]
    Prune(PruneArgs),
    #[command(about = "Clear the .maiodb journal")]
    Clean,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    #[arg(short, long, help = "Target output directory for baked assets", default_value = "target/assets")]
    pub out_dir: PathBuf,

    #[arg(short, long, help = "Limit worker thread count")]
    pub jobs: Option<usize>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(short, long, help = "Filter by status: pending | completed | all", default_value = "all")]
    pub filter: String,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    #[arg(help = "32-character hexadecimal AssetId")]
    pub asset_id: String,
}

#[derive(Args, Debug)]
pub struct PruneArgs {
    #[arg(short, long, help = "Assets directory", default_value = "target/assets")]
    pub dir: PathBuf,
}
