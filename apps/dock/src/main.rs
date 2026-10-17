// Single responsibility: dock overlay entry point (config only).
//
// The whole layer-shell host — connection, raster loop, glass-island
// effect uploads, pointer bridge, hot-reload — lives at the engine level
// in `riode_platform_wayland::runner::overlay_run`. This binary only
// declares which overlay it is: a bottom-fixed interactive bar whose glass
// rides the parent surface.
//
// The view is HOT: `examples/dock/view.rs` (mounted into the
// `riode-hot-gen-ui` cdylib) is watched, rebuilt, and swapped live without
// restarting this process.
//
// Run (dev): `dev example dock --overlay`. Plain `cargo run` works only
// if `target/release/libriode_shared.so` + the UI image already exist AND
// this binary was built with `-C prefer-dynamic`.

extern crate riode_shared as riode;

use riode_platform_wayland::runner::{
    overlay_run, GlassMode, HotConfig, OverlayConfig, SurfacePolicy,
};

const INTER: &[u8] = include_bytes!("../../../crates/core/riode/assets/inter-var.ttf");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Standalone hot-reload: the UI is this package's own cdylib
    // (`src/lib.rs`), watched at `apps/dock/src` — no `examples/` dir.
    // Run: `dev apps dock`.
    overlay_run(&OverlayConfig {
        namespace: "dock",
        hot: HotConfig {
            package: "dock",
            example: None,
            watch_dir: Some("apps/dock/src"),
            companion_set: "dock",
        },
        surface: SurfacePolicy::BottomFixed,
        interactive: true,
        glass: GlassMode::ParentSurface,
        font_bytes: INTER,
    })
}
