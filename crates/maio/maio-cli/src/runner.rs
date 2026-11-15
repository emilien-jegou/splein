// Responsibility: Resolves workspace context and coordinates generic CLI subcommand execution.

use crate::args::{BuildArgs, Cli, Command, InspectArgs, ListArgs, PruneArgs};
use maio::Maiodb;
use maio_compiler::{Compiler, ProcessorRegistry};
use maio_core::AssetId;
use std::path::Path;
use tracing::instrument;

#[instrument(skip_all)]
pub fn execute(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let root = match cli.root {
        Some(r) => r,
        None => {
            let cwd = std::env::current_dir()?;
            Maiodb::find_workspace_root(&cwd)
                .ok_or("Could not locate workspace root with [workspace] in Cargo.toml")?
        }
    };

    let db = Maiodb::open_workspace(&root)?;

    match cli.command {
        Command::Build(args) => run_build(&db, &root, args),
        Command::Status => run_status(&db),
        Command::List(args) => run_list(&db, args),
        Command::Inspect(args) => run_inspect(&db, args),
        Command::Prune(args) => run_prune(&db, &root, args),
        Command::Clean => run_clean(&db),
    }
}

fn run_build(db: &Maiodb, root: &Path, args: BuildArgs) -> Result<(), Box<dyn std::error::Error>> {
    let snap = db.snapshot()?;
    let pending: Vec<_> = snap.pending().into_iter().map(|(id, task)| (id, task.clone())).collect();

    if pending.is_empty() {
        println!("All assets up to date (0 pending).");
        return Ok(());
    }

    let out_dir = if args.out_dir.is_absolute() { args.out_dir } else { root.join(args.out_dir) };
    
    // Generic runner: registers any built-in or plugin processors
    let registry = ProcessorRegistry::new();
    let mut compiler = Compiler::new(out_dir, registry);
    if let Some(jobs) = args.jobs {
        compiler.set_jobs(jobs);
    }

    let report = maio_compiler::compile_and_commit(db, &compiler, &pending)?;
    println!("{}", report.summary());
    if !report.is_ok() {
        for fail in report.failed {
            eprintln!("FAIL [{}]: {}", fail.id, fail.error);
        }
        std::process::exit(1);
    }
    Ok(())
}

fn run_list(db: &Maiodb, args: ListArgs) -> Result<(), Box<dyn std::error::Error>> {
    let snap = db.snapshot()?;
    println!("{:<10} {:<16} {:<10} {:<10} {:<32} {}", "ID", "KIND", "STATUS", "SIZE", "CALLSITE", "SOURCE");
    println!("{}", "-".repeat(100));

    for (id, task) in &snap.tasks {
        let is_completed = snap.artifacts.contains_key(id);
        if args.filter == "pending" && is_completed { continue; }
        if args.filter == "completed" && !is_completed { continue; }

        let (status, size_str) = match snap.artifacts.get(id) {
            Some(meta) => ("✓ Done", format_size(meta.size)),
            None => ("⚙ Pending", "-".into()),
        };

        let callsite = task.callsite.as_deref().unwrap_or("<unknown>");
        let src = task.source_path().map(|p| p.display().to_string()).unwrap_or_else(|| "<inline>".into());
        println!("{:<10} {:<16} {:<10} {:<10} {:<32} {}", id.short_hex(), task.kind(), status, size_str, callsite, src);
    }
    Ok(())
}

fn run_status(db: &Maiodb) -> Result<(), Box<dyn std::error::Error>> {
    let snap = db.snapshot()?;
    let pending = snap.pending();
    println!("Manifest:   {}", db.manifest_path().display());
    println!("Tasks:      {} total, {} pending", snap.tasks.len(), pending.len());
    println!("Artifacts:  {} completed", snap.artifacts.len());
    Ok(())
}

fn run_inspect(db: &Maiodb, args: InspectArgs) -> Result<(), Box<dyn std::error::Error>> {
    let id: AssetId = args.asset_id.parse()?;
    let snap = db.snapshot()?;
    match snap.tasks.get(&id) {
        Some(task) => {
            println!("AssetId:    {}", id.as_hex());
            println!("Kind:       {}", task.kind());
            println!("Source:     {:?}", task.source_path());
            if let Some(meta) = snap.artifacts.get(&id) {
                println!("Status:     Completed (size: {})", format_size(meta.size));
            } else {
                println!("Status:     Pending");
            }
            Ok(())
        }
        None => Err(format!("AssetId {} not found", args.asset_id).into()),
    }
}

fn run_prune(db: &Maiodb, root: &Path, args: PruneArgs) -> Result<(), Box<dyn std::error::Error>> {
    let snap = db.snapshot()?;
    let dir = if args.dir.is_absolute() { args.dir } else { root.join(args.dir) };
    if !dir.exists() { return Ok(()); }

    let mut removed = 0;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "bin") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(id) = stem.parse::<AssetId>() {
                if !snap.tasks.contains_key(&id) {
                    std::fs::remove_file(&path)?;
                    removed += 1;
                }
            }
        }
    }
    println!("Pruned {removed} orphaned artifact(s) from {}", dir.display());
    Ok(())
}

fn run_clean(db: &Maiodb) -> Result<(), Box<dyn std::error::Error>> {
    db.clear()?;
    println!("Cleared manifest journal at {}", db.manifest_path().display());
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{bytes} B") }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
}
