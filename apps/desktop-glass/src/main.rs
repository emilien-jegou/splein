// Single responsibility: desktop-glass overlay entry point (config only).
//
// The whole layer-shell host — connection, raster loop, glass-island
// effect uploads, hot-reload — lives at the engine level in
// `riode_platform_wayland::runner::overlay_run`. This binary only
// declares which overlay it is: a fullscreen non-interactive demo where
// each glass island gets its own layer surface (the plain card proves
// exclusion by absence).
//
// The view is HOT: `examples/desktop_glass/view.rs` (mounted into the
// `riode-hot-gen-ui` cdylib) is watched, rebuilt, and swapped live without
// restarting this process. `RIODE_HOT_EXAMPLE` (exported by
// `example.sh --overlay`) swaps which example mounts — e.g.
// `prism_only` — and must match the example the dylib was built with.
//
// Run (dev): `dev example desktop-glass --overlay`, or
// `dev example <name> wgpu --overlay` for other overlay-shaped views.
// Plain `cargo run` works only if `target/release/libriode_shared.so` +
// the UI image already exist AND this binary was built with
// `-C prefer-dynamic`.

extern crate riode_shared as riode;

use riode_platform_wayland::runner::{
    overlay_run, GlassMode, HotConfig, OverlayConfig, SurfacePolicy,
};

const INTER: &[u8] = include_bytes!("../../../crates/core/riode/assets/inter-var.ttf");

/// The generative example to mount: `example.sh --overlay` exports
/// `RIODE_HOT_EXAMPLE` before exec'ing this host (the boot build and
/// every loop rebuild must target the same example the dylib was built
/// with). Bare runs keep the five-island demo.
fn hot_example() -> &'static str {
    match std::env::var("RIODE_HOT_EXAMPLE") {
        Ok(v) if !v.is_empty() => Box::leak(v.into_boxed_str()),
        _ => "desktop_glass",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generative hot-reload: the UI is `examples/<example>/` mounted
    // through the `riode-hot-gen-ui` cdylib. Run: `dev example
    // desktop-glass --overlay` (or `dev example <name> --overlay`).
    overlay_run(&OverlayConfig {
        namespace: "desktop-glass",
        hot: HotConfig {
            package: "riode-hot-gen-ui",
            example: Some(hot_example()),
            watch_dir: None,
            companion_set: "desktop-glass",
        },
        surface: SurfacePolicy::Fullscreen,
        interactive: false,
        glass: GlassMode::IslandLayers,
        font_bytes: INTER,
    })
}
