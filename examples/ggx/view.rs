// GGX demo (A2): the microfacet specular glint is a separate stack item.
//
// A/B discipline (v2, same instrument as the fresnel demo): every card
// of a pair sits over IDENTICAL backdrop content. The backdrop is a
// static field of vertical colour stripes whose period equals the
// card-pair offset (card 220 + gap 48 = 268 px), so the right card's
// food is the left card's food translated by exactly one period —
// pixel-for-pixel identical. The field is also y-invariant, so every
// row (same x) samples the same content too. Nothing drifts; the only
// difference within a pair is one GGX knob or one stack item.
//
// ISOLATION: none of these cards run Rim or Fresnel — the only edge
// light is the GGX lobe itself. OFF is therefore perfectly flat at the
// rim; ON is a soft white patch riding the bevel band on the lit side.
// If you see any rim light on an OFF card, that is a bug, not the
// baseline.
//
// DARK food: the stripes are darkened (~35% of the fresnel demo's) so
// the additive lobe reads as a brightening instead of clipping against
// an already-white backdrop. Same hues, same period, just dimmer.
//
// Rows (gain 80 — the lobe math needs it; aligned spec/gain ≈ 0.003 at
// roughness 0.6 on this lens, so 80 lands ≈ +0.25, bright but
// unclipped on the dark food; at roughness 0.2 the same gain gives
// ≈ +0.005 — invisible by construction):
//   - "GGX OFF" vs "GGX ON" (gain 0 vs 80, both roughness 0.6): ON must
//     show a soft bright patch on the upper-left bevel, broad and milky
//     (several px wide), fading inward — never a crisp 1px line, never
//     a face lift. OFF is flat there.
//   - "ROUGHNESS 0.2" vs "ROUGHNESS 0.6": same gain, same light — 0.2
//     is effectively invisible (≈ +0.005), 0.6 is clearly visible
//     (≈ +0.25) and broader. Unmissable by construction: one card flat,
//     one card glinting.
//   - "LIGHT TOP-LEFT" vs "LIGHT TOP-RIGHT": same gain, same roughness
//     — the patch mirrors to the opposite rim. (Named TOP-LEFT/RIGHT
//     because the key light is above the card: light y is negative,
//     and y points DOWN in surface coords, so -0.35 means up. There is
//     no bottom-lit variant — the lobe lives on the upper bevel.)
//     There is no Rim streak to confuse it with (Rim is absent on
//     every card). NOTE: TOP-LEFT intentionally matches the ROUGHNESS
//     0.6 card's config (same gain/roughness/light) — they SHOULD look
//     identical. That is the cross-row consistency check, not a
//     redundant row: roughness controls width/brightness, light
//     controls position.
//   - "NO REFRACT" vs "WITH REFRACT": without Refract nothing publishes
//     the bevel profile (`vv` stays at its identity 0), so the gate
//     kills the glint entirely — the NO REFRACT card must look exactly
//     like GGX OFF (flat). The separation proof.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{ColorStop, EffectStack, Frost, Ggx, GlassEffect, LinearGradient, Point, Refract};
use riode::internal::group;
use riode::prelude::*;

#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

pub const HOT_TITLE: &str = "Aurora — GGX";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 1180;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0x2F6FEB;
const LABEL_COLOR: u32 = 0xFFFFFF;
const INK_COLOR: u32 = 0xC81E3A;

/// Gain baseline: the lobe needs it (see header). Every lit card uses
/// the same gain so rows compare exactly one knob.
const GAIN: f32 = 80.0;

/// The card lens: the clear preset's base look with the per-card GGX
/// knobs applied — the only source of row differences. `gain` is the
/// specular gain (0 = off); `roughness` 0 keeps the shader default 0.2;
/// `light_x` 0 keeps the default key light, ±0.85 steers the patch
/// hard top-left / top-right (light = [light_x, -0.35, 0.4],
/// normalized on the CPU; y is DOWN, so -0.35 is above the card).
fn lens_card(gain: f32, roughness: f32, light_x: f32) -> GlassEffect {
    let mut style = GlassPreset::clear().style();
    style.glass = style.glass.ggx(gain);
    if roughness > 0.0 {
        style.glass = style.glass.ggx_roughness(roughness);
    }
    if light_x.abs() > 0.0 {
        style.glass = style.glass.ggx_light_dir([light_x, -0.35, 0.4]);
    }
    style.into_effect()
}

#[component]
fn Card(label: &'static str, backdrop_effects: Option<EffectStack>) -> impl IntoElement {
    let backdrop_effects = backdrop_effects.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={8.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                backdrop_effects={backdrop_effects}
                alignment="center"
                distribution="center"
            >
                <Text size={48.0} weight={700u16} color={Color::hex(INK_COLOR)}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

/// The static backdrop food: vertical saturated stripes, y-invariant,
/// darkened to ~35% so the additive glint reads. Four stripes of 67 px
/// make one 268 px period — exactly the card-pair offset (card 220 +
/// gap 48) — so both cards of a pair sample the same content, and every
/// row (same x) samples the same content too.
fn stripes() -> Vec<Element> {
    const W: f32 = 67.0;
    // Same hues as the fresnel instrument, darkened for an additive lobe.
    const COLORS: [u32; 4] = [0x501111, 0x563A0D, 0x113A26, 0x152D56];
    let mut out = Vec::new();
    for i in 0..20 {
        out.push(
            group()
                .width(W)
                .height(1180.0)
                .absolute()
                .fill(Color::hex(COLORS[i % 4]))
                .transform(Transform::from_translation(i as f32 * W, 0.0))
                .into_element(),
        );
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
            fill={page_bg()}
            alignment="center"
            distribution="center"
        >
            {stripes()}
            <Group direction="vertical" width="fill" gap={28.0} alignment="center" distribution="center" z_index={2} padding="20 0">
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="GGX OFF"
                        backdrop_effects=[lens_card(0.0, 0.6, 0.0), Refract(), Frost(), Ggx()]
                    />
                    <Card
                        label="GGX ON"
                        backdrop_effects=[lens_card(GAIN, 0.6, 0.0), Refract(), Frost(), Ggx()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="ROUGHNESS 0.2"
                        backdrop_effects=[lens_card(GAIN, 0.2, 0.0), Refract(), Frost(), Ggx()]
                    />
                    <Card
                        label="ROUGHNESS 0.6"
                        backdrop_effects=[lens_card(GAIN, 0.6, 0.0), Refract(), Frost(), Ggx()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="LIGHT TOP-LEFT"
                        backdrop_effects=[lens_card(GAIN, 0.6, -0.85), Refract(), Frost(), Ggx()]
                    />
                    <Card
                        label="LIGHT TOP-RIGHT"
                        backdrop_effects=[lens_card(GAIN, 0.6, 0.85), Refract(), Frost(), Ggx()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="NO REFRACT"
                        backdrop_effects=[lens_card(GAIN, 0.6, 0.0), Frost(), Ggx()]
                    />
                    <Card
                        label="WITH REFRACT"
                        backdrop_effects=[lens_card(GAIN, 0.6, 0.0), Refract(), Frost(), Ggx()]
                    />
                </Group>
            </Group>
        </Group>
    }
}

fn page_bg() -> Fill {
    // Never visible (the stripes cover the window); a solid-ish floor
    // for any rounding gap.
    Fill::LinearGradient(LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(0.0, 1180.0),
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Color::hex(0x2F6FEB),
            },
            ColorStop {
                position: 1.0,
                color: Color::hex(0x7B2FF7),
            },
        ],
    })
}
