// Single responsibility: effect-order composition demo on the two
// effect stacks — `layer_effects` (the sheet, CSS `filter`) and
// `backdrop_effects` (sampled backdrop + lens, CSS
// `backdrop-filter`). Order within each list is execution order
// (bottom → top). `=[...]` bracket syntax throughout.
// NOTE: run with the wgpu engine (`-- wgpu` / `example effect_stack
// wgpu`); tiny-skia ignores glass layers by design, so every pair
// renders identically there.
//   - "SAT → GLASS": layer desaturates the sheet (grey text); the lens
//     refracts the still-colored backdrop — grey text, colored lens.
//   - "GLASS → SAT": backdrop stack runs glass then Saturate, so the
//     finished lens card also desaturates — everything grey.
//   - "BLUR → GLASS": layer blurs the sheet — soft text, crisp lens.
//   - "GLASS → BLUR": backdrop stack runs glass then Blur — lens and
//     text both soft.
//   - "LAYER SAT": layer_effects only — no glass; grey text sheet, the
//     split backdrop under it stays red/blue (intentional no-glass
//     control card).
//   - "BACKDROP GLASS+SAT": backdrop stack only — glass then Saturate
//     over the sampled backdrop; no sheet content.
// Stage-fold rows (the regular preset — strong grade/rim knobs; one
// variable per step down the ladder):
//   - "FULL (regular)": glass only — the canonical chain (count 0).
//   - "OPTICS ONLY": refract+frost+disperse — grade/rim removed.
//   - "OPTICS + GRADE": grade family returns; rim still absent.
//   - "OPTICS + GRADE + RIM": rim returns — matches FULL statically.
//   - "GRADE BEFORE OPTICS": same items, grade listed first — frost
//     resets `col`, so the early grade must be swallowed (order proof);
//     its right-hand control repeats OPTICS + GRADE on the same row.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{
    Ambient, Blur, Dim, Disperse, EffectStack, Frost, GlassEffect, Refract, Rim, Saturate,
    Saturation, Tint, Tone, Wash,
};
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
pub const HOT_HEIGHT: u32 = 1584;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0xC81E3A;

/// The preset lens as one bracket element (hot and release resolve the
/// same preset values).
fn glass() -> GlassEffect {
    GlassPreset::clear().style().into_effect()
}

/// The regular preset: strong grade (sat 1.49, wash 0.68, ambient 0.18)
/// and a visible rim (0.23) — the stage-fold rows need observable knobs.
fn glass_regular() -> GlassEffect {
    GlassPreset::regular().style().into_effect()
}

#[component]
fn Card(
    label: &'static str,
    layer_effects: Option<EffectStack>,
    backdrop_effects: Option<EffectStack>,
) -> impl IntoElement {
    let layer_effects = layer_effects.unwrap_or_default();
    let backdrop_effects = backdrop_effects.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={200.0}
                height={160.0}
                radius={24.0}
                layer_effects={layer_effects}
                backdrop_effects={backdrop_effects}
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
/// 220×160 cards, columns at x≈397/665, rows at y≈158/386/614/842/1070/1298.
fn backdrop_blobs() -> Vec<Element> {
    let cards = [
        (397.0, 158.0),
        (665.0, 158.0),
        (397.0, 386.0),
        (665.0, 386.0),
        (397.0, 614.0),
        (665.0, 614.0),
        (397.0, 842.0),
        (665.0, 842.0),
        (397.0, 1070.0),
        (665.0, 1070.0),
        (397.0, 1298.0),
        (665.0, 1298.0),
    ];
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
                    <Card label="SAT → GLASS" layer_effects=[Saturate::new(0.0)] backdrop_effects=[glass()] />
                    <Card label="GLASS → SAT" layer_effects=[Saturate::new(0.0)] backdrop_effects=[glass(), Saturate::new(0.0)] />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="BLUR → GLASS" layer_effects=[Blur::new(6.0)] backdrop_effects=[glass()] />
                    <Card label="GLASS → BLUR" layer_effects=[Blur::new(6.0)] backdrop_effects=[glass(), Blur::new(6.0)] />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="LAYER SAT" layer_effects=[Saturate::new(0.0)] backdrop_effects=[glass()] />
                    <Card label="BACKDROP GLASS+SAT" backdrop_effects=[glass(), Saturate::new(0.0)] />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="FULL (regular)" backdrop_effects=[glass_regular()] />
                    <Card label="OPTICS ONLY" backdrop_effects=[glass_regular(), Refract(), Frost(), Disperse()] />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="OPTICS + GRADE" backdrop_effects=[glass_regular(), Refract(), Frost(), Disperse(), Saturation(), Tone(), Wash(), Dim(), Tint(), Ambient()] />
                    <Card label="OPTICS + GRADE + RIM" backdrop_effects=[glass_regular(), Refract(), Frost(), Disperse(), Saturation(), Tone(), Wash(), Dim(), Tint(), Ambient(), Rim()] />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card label="GRADE BEFORE OPTICS" backdrop_effects=[glass_regular(), Saturation(), Tone(), Wash(), Dim(), Tint(), Ambient(), Refract(), Frost(), Disperse()] />
                    <Card label="OPTICS + GRADE (control)" backdrop_effects=[glass_regular(), Refract(), Frost(), Disperse(), Saturation(), Tone(), Wash(), Dim(), Tint(), Ambient()] />
                </Group>
            </Group>
        </Group>
    }
}
