// Single responsibility: `riode_item_v1` client plumbing for islands.
//
// Binds the plugin's hint manager (v2: raw material, no preset names),
// mints one item object per island surface, and uploads the per-commit
// material + shape hints (the *what* + the shape). Double-buffered:
// requests apply on the island surface's next commit, which for a
// top-level layer surface is its own commit.
//
// Client code is generated at compile time from
// `protocols/riode-item-v1.xml` (a copy of the plugin's protocol; keep
// the two in sync) via the `wayland-scanner` proc-macro — no codegen
// binary needed.
//
// Embedder contract: the event-loop `State` must implement
// `Dispatch<RiodeItemManagerV1, ()>` — a MANUAL empty impl, because the
// manager sends a `finished` event and `delegate_noop!` traps any received
// event — and `Dispatch<RiodeItemV1, ()>` (no events; `delegate_noop!` is
// fine).

pub mod generated {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    pub mod client {
        //! Client-side API of the riode_item_v1 protocol.
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocols/riode-item-v1.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("protocols/riode-item-v1.xml");
    }
}

pub use generated::client::riode_item_manager_v1::RiodeItemManagerV1;
pub use generated::client::riode_item_v1::RiodeItemV1;

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Dispatch, QueueHandle};

/// Bind the item-hint manager (version 2: raw material, no preset
/// names). Fails when the riode-glass plugin isn't loaded, or speaks
/// only v1 — the caller keeps running without hints, same as a missing
/// manager. There is deliberately no v1 fallback: the preset-name
/// channel is dead.
pub fn bind_item_manager<State>(
    globals: &GlobalList,
    qh: &QueueHandle<State>,
) -> Result<RiodeItemManagerV1, BindError>
where
    State: Dispatch<RiodeItemManagerV1, ()> + 'static,
{
    globals.bind(qh, 2..=2, ())
}

/// Mint one item object for an island surface. At most one active item per
/// surface (the protocol raises `item_exists` otherwise).
pub fn item_for_surface<State>(
    manager: &RiodeItemManagerV1,
    qh: &QueueHandle<State>,
    surface: &WlSurface,
) -> RiodeItemV1
where
    State: Dispatch<RiodeItemV1, ()> + 'static,
{
    manager.get_item(surface, qh, ())
}

/// Upload one island's packed glass frame (v4 wire: header + the 736
/// uniform floats, little-endian u32 words). Idempotent: sending the
/// same bytes every commit is harmless (pending state is simply
/// overwritten). An empty slice clears the hint (plain-blur fallback
/// applies).
pub fn upload_glass_frame(item: &RiodeItemV1, words: &[u32]) {
    if words.is_empty() {
        item.unset_glass();
        return;
    }
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    item.set_glass(bytes);
}

/// Uniform stream per stage, in f32s. The wire always carries the
/// full fixed 8-vec4 block the ABI's `CustomUniforms` declares (shorter
/// streams are zero-padded); longer ones cannot be represented and the
/// client skips the stage with a warning before the helper drops it.
const EFFECT_UNIFORMS_MAX: usize = 32;

/// Upload one island's ordered effect stack (its `backdrop_effects`):
/// clears every previously-latched stage, sends one `set_effect` per
/// custom stage (WGSL composed as ABI header + module, uniforms padded
/// to whole vec4s and packed little-endian), then declares where the
/// glass frame sits among the stages. Builtin stages other than Glass
/// have no wire representation yet and are skipped once with a warning;
/// hints never raise protocol errors. Always call alongside
/// [`upload_glass_frame`] in the same commit batch.
pub fn upload_effect_stages(item: &RiodeItemV1, effects: Option<&riode_shared::foundation::EffectStack>) {
    use riode_shared::foundation::effect::shaders::EFFECT_ABI_WGSL;
    use riode_shared::foundation::effect::{BackdropNeed, BuiltinStage, EffectShader};
    use std::sync::atomic::{AtomicBool, Ordering};

    item.clear_effects();
    // -1 = no Glass in the stack (every latched stage is a backdrop stage);
    // any Glass at index i overwrites it below.
    let mut glass_index = -1i32;
    if let Some(stack) = effects {
        for (idx, e) in stack.0.iter().enumerate() {
            let idx = idx as i32;
            match e.shader() {
                EffectShader::Custom(c) => {
                    // Full module on the wire: ABI definitions first so
                    // the plugin's transpile sees one complete unit.
                    if c.wgsl.contains('\0') {
                        static WARNED_NUL: AtomicBool = AtomicBool::new(false);
                        if !WARNED_NUL.swap(true, Ordering::Relaxed) {
                            tracing::warn!("riode-wayland: custom effect WGSL contains NUL (skipped)");
                        }
                        continue;
                    }
                    let mut floats = c.uniforms.clone();
                    if floats.len() <= EFFECT_UNIFORMS_MAX {
                        floats.resize(EFFECT_UNIFORMS_MAX, 0.0);
                    }
                    if floats.len() > EFFECT_UNIFORMS_MAX {
                        static WARNED_LONG: AtomicBool = AtomicBool::new(false);
                        if !WARNED_LONG.swap(true, Ordering::Relaxed) {
                            tracing::warn!(
                                "riode-wayland: custom effect uniforms exceed {EFFECT_UNIFORMS_MAX} f32s (stage skipped)"
                            );
                        }
                        continue;
                    }
                    let mut uniforms = Vec::with_capacity(floats.len() * 4);
                    for u in &floats {
                        uniforms.extend_from_slice(&u.to_le_bytes());
                    }
                    let (need, sigma) = match c.backdrop {
                        BackdropNeed::None => {
                            (generated::client::riode_item_v1::BackdropNeed::None, 0.0)
                        }
                        BackdropNeed::ReadSharp => (
                            generated::client::riode_item_v1::BackdropNeed::ReadSharp,
                            0.0,
                        ),
                        BackdropNeed::ReadBlurred { sigma } => (
                            generated::client::riode_item_v1::BackdropNeed::ReadBlurred,
                            sigma as f64,
                        ),
                    };
                    item.set_effect(
                        idx,
                        format!("{EFFECT_ABI_WGSL}\n{}", c.wgsl),
                        uniforms,
                        need,
                        sigma,
                    );
                }
                EffectShader::Builtin(BuiltinStage::Glass(_)) => glass_index = idx,
                other => {
                    static WARNED_BUILTIN: AtomicBool = AtomicBool::new(false);
                    if !WARNED_BUILTIN.swap(true, Ordering::Relaxed) {
                        tracing::warn!(
                            "riode-wayland: builtin stage {other:?} has no wire representation (skipped)"
                        );
                    }
                }
            }
        }
    }
    item.set_glass_stage(glass_index);
}

/// Upload one island's shape hint. Idempotent: sending the same values
/// every commit is harmless (pending state is simply overwritten).
///
/// `rect` and `radii` are island-surface-local logical px — for our
/// islands the surface IS the rect, so callers pass `(0, 0, w, h)`.
pub fn upload_item_shape(
    item: &RiodeItemV1,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    radii: [f64; 4],
) {
    if w <= 0.0 || h <= 0.0 {
        item.unset_shape();
        return;
    }
    let radii = radii.map(|r| r.max(0.0));
    item.set_shape(x, y, w, h, radii[0], radii[1], radii[2], radii[3]);
}
