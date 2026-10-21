// Single responsibility: effect-order composition demo — identical cards
// whose `effects` stacks differ only in stage order, over one shared
// colorful backdrop. Order is execution order (bottom → top).
// NOTE: run with the wgpu engine (`-- wgpu` / `example effect_stack
// wgpu`); tiny-skia ignores glass layers by design, so every pair
// renders identically there.
//   - "SAT → GLASS": text desaturates first, then the lens refracts the
//     still-colored backdrop — grey text, colored lens.
//   - "GLASS → SAT": the finished card (lens + text) desaturates —
//     everything grey, lens included.
//   - "BLUR → GLASS": text blurs first, then the lens refracts the sharp
//     backdrop — soft text, crisp lens.
//   - "GLASS → BLUR": the finished card blurs — lens and text both soft.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{Blur, Effect, EffectStack, Filter};
use riode::internal::group;
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Effect Stack";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 900;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0xC81E3A;

fn glass_fx() -> EffectStack {
    GlassPreset::clear().style().into_effects()
}

fn sat_fx() -> EffectStack {
    Filter::saturate(0.0).into_effects()
}

fn blur_fx() -> EffectStack {
    EffectStack::from(vec![Blur::new(6.0).boxed()])
}

#[component]
fn Card(label: &'static str, effects: EffectStack) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                effects={effects}
                alignment="center"
                distribution="center"
            >
                <Text size={48.0} weight={700u16} color={Color::hex(INK_COLOR)}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

/// Backdrop behind every card: an identical red-left / blue-right split
/// (opaque), slightly larger than the card so frost tails also sample
/// colour. Identical per card on purpose — order differences must read
/// from the effect stack, never from the backdrop. Card grid estimates:
/// 220×160 cards, columns at x≈396/664, rows at y≈272/484.
fn backdrop_blobs() -> Vec<Element> {
    let cards = [(396.0, 272.0), (664.0, 272.0), (396.0, 484.0), (664.0, 484.0)];
    let mut out = Vec::new();
    for (x0, y0) in cards {
        for (dx, w, fill) in [
            (-40.0, 150.0, Color::hex(0xE5484D)),
            (110.0, 150.0, Color::hex(0x3B82F6)),
        ] {
            out.push(
                group()
                    .width(w)
                    .height(280.0)
                    .absolute()
                    .fill(fill)
                    .transform(Transform::from_translation(x0 + dx, y0 - 60.0))
                    .into_element(),
            );
        }
    }
    out
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Fill::Solid(Color::hex(BG))}
            alignment="center"
            distribution="center"
            gap={40.0}
        >
            {backdrop_blobs()}
            <Group direction="vertical" gap={36.0} alignment="center" distribution="center" z_index={2}>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="SAT → GLASS" effects={sat_fx().then(glass_fx())} />
                    <Card label="GLASS → SAT" effects={glass_fx().then(sat_fx())} />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="BLUR → GLASS" effects={blur_fx().then(glass_fx())} />
                    <Card label="GLASS → BLUR" effects={glass_fx().then(blur_fx())} />
                </Group>
            </Group>
        </Group>
    }
}
