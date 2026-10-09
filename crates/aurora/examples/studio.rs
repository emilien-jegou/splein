// Single responsibility: Switchable master binary hosting every demo in one window.
//
// F5 opens the grub-style demo menu, Enter/click switches live in the
// current window, F6 toggles quad combine, F7 cycles raster engines,
// Esc goes back / exits. Optional positional demo id selects the start
// demo: `cargo run --example studio -- motion-press`.

#[path = "common/mod.rs"]
mod common;
#[path = "common/registry.rs"]
mod registry;

use common::ExampleCli;

fn main() {
    let cli = ExampleCli::parse(1280, 800);
    cli.init_telemetry();
    // First positional arg that names a demo wins as the start demo;
    // anything else is the engine selector the CLI already parsed.
    let start = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .find(|a| registry::all().iter().any(|d| d.id == a.as_str()))
        .unwrap_or_else(|| "text-smooth".to_string());
    // `all()` allocates per call; leak one copy for the 'static id.
    let initial: &'static str = Box::leak(start.into_boxed_str());
    registry::run_demo(cli, initial);
}
