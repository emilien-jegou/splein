// Single responsibility: compositor-passthrough warp demo (Phase 4.5).
//
// Three glass islands with identical content but different warp order:
//   - "WARP → GLASS": Warp runs on the backdrop walk before the lens;
//     it displaces the capture the lens then blurs/refracts — the
//     refraction position in the pipeline (surface-shape proof).
//   - "GLASS → WARP": Warp runs on the finished lens card; the ripple
//     sits flat over the card, pixel-regular, ignoring lens geometry.
//   - "GLASS ONLY" (control): no warp — if this island loses its lens
//     or both warps vanish, the pipeline broke, not the order.
//
// `Warp` is a real foundation stage (`EffectShader::Custom`): its module
// travels program → riode-glass plugin over `riode-item-v1` (the client
// prepends the effect ABI header), transpiles in the in-tree glue (naga →
// GLSL ES 3.00) and runs as a GL stage there. The wgpu/tiny-skia engines
// skip custom stages by design, so the demo must run as the overlay
// (`example compositor_warp --overlay`): the plugin owns the backdrop
// path. The windowed preview shows layout/material only — no warp there
// is expected, not a bug.
// Islands are static on purpose: one variable per observation — the
// only difference between the two rows is stack position.

use riode::foundation::{Amplitude, EffectStack, Frequency, Warp};
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release preview (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never mirrored.
/// The overlay host (`apps/desktop-glass`) sizes from layer-shell configure
/// and always paints transparent, ignoring HOT_BG.
pub const HOT_TITLE: &str = "Compositor warp";
pub const HOT_WIDTH: u32 = 1040;
pub const HOT_HEIGHT: u32 = 420;
pub const HOT_BG: u32 = 0x14161C;

const LABEL_COLOR: u32 = 0xFFFFFF;

/// The preset lens as one stack element (hot and release resolve the
/// same preset values) — `.then` chains keep index order = execution order.
fn glass() -> GlassPreset {
    GlassPreset::clear()
}

/// Custom backdrop stage: warp before the lens (bends the desktop the
/// lens then frosts) or after it (ripples the finished card flat) —
/// stack position decides, nothing else about the stage changes between
/// the two rows.
fn warp() -> Warp {
    Warp::sine(Frequency::per_pixel(0.05), Amplitude::px(18.0))
}

#[component]
fn IslandCard(label: &'static str, material: Option<EffectStack>) -> impl IntoElement {
    let material = material.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={280.0}
                height={200.0}
                radius={24.0}
                material={material}
                alignment="center"
                distribution="center"
            >
                <Text size={44.0} weight={700u16} color={Color::WHITE}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            alignment="center"
            distribution="center"
            gap={56.0}
        >
            <IslandCard label="WARP → GLASS" material={warp().then(glass().style().into_effect())} />
            <IslandCard label="GLASS → WARP" material={glass().style().into_effect().then(warp())} />
            <IslandCard label="BLOCK WARP" material={Warp::sine(Frequency::per_pixel(0.05), Amplitude::px(18.0)).into_surface()} />
            <IslandCard label="GLASS ONLY" material={glass().style().into_effects()} />
        </Group>
    }
}
