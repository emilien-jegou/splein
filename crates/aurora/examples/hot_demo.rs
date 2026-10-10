// Single responsibility: Windowed host for remote-UI hot-reload iteration.
//
// Runs the `aurora-hot-ui` cdylib through `RemoteReloadExtension`: edit
// `crates/aurora-hot-ui/src/lib.rs`, save, and the window rebuilds from the fresh library
// with every counter, toggle, and spring intact. `F6` (or the panel Reload button) forces
// a rebuild from the current library without touching sources.
//
// Usage: `cargo run -p aurora --example hot_demo`
// Then edit `crates/aurora-hot-ui/src/lib.rs` (try the `hero` fill) and save.

use aurora::hot_reload::{DevLoopConfig, RemoteReloadExtension};
use aurora::prelude::*;
use common::ExampleCli;
use std::path::PathBuf;

mod common;

pub const WIDTH: u32 = 500;
pub const HEIGHT: u32 = 560;

/// Workspace root derived from this crate's manifest dir (`crates/aurora` → root).
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    let ext = RemoteReloadExtension::new(DevLoopConfig {
        package: "aurora-hot-ui".to_string(),
        workspace_root: workspace_root(),
        watch_dir: None,
        lib_path: None,
        auto_build: true,
    });
    cli.app_config("Aurora — Hot Reload Demo [edit aurora-hot-ui, F6 rebuild]")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::WHITE)
        .extension(ext)
        .run(placeholder);
}

/// Shown until the first UI build finishes loading.
fn placeholder() -> impl IntoElement {
    group()
        .width(500.0)
        .height(560.0)
        .fill(Color::WHITE)
        .children([text("building hot-ui…").size(16.0).color(Color::hex(0x6B7280))])
}
