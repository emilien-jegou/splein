// Fresnel demo (A1): the Schlick edge glint is a separate stack item.
//
// A/B discipline (v2): every card of a pair sits over IDENTICAL
// backdrop content. The backdrop is a static field of vertical colour
// stripes whose period equals the card-pair offset (card 220 + gap 48
// = 268 px), so the right card's food is the left card's food
// translated by exactly one period — pixel-for-pixel identical. The
// field is also y-invariant, so every row sees the same content and
// cross-row comparisons are valid too. Nothing drifts; the only
// difference within a pair is one knob on the material:
//   - "FRESNEL OFF" vs "FRESNEL ON" (gain 0 vs 2.0): ON must show a
//     bright band hugging the bevel just inside the card edge (hottest
//     at the corners, cut before the very outer edge) plus a uniform
//     milky face lift.
//   - "GAIN 1.0" vs "GAIN 4.0": same band, same placement, brightness
//     only.
//   - "NO REFRACT" vs "WITH REFRACT": without Refract nothing publishes
//     the tipped normal, so the glint degenerates to a constant F0
//     lift — a flat milky card, no rim band. The separation proof.
//   - "POWER 1.5" vs "F0 0.6": spread (glint reaching inward) vs floor
//     (whole-face lift) of the same glint — both at gain 1.0 so the
//     shapes stay unclipped (at gain 2.0 both clipped to flat white and
//     the difference vanished — gate round 2).
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{ColorStop, EffectStack, Frost, EffectFresnel, GlassEffect, LinearGradient, Point, Refract, Rim};
use riode::internal::group;
use riode::prelude::*;

#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

pub const HOT_TITLE: &str = "Aurora — Fresnel";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 1180;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0x2F6FEB;
const LABEL_COLOR: u32 = 0xFFFFFF;
const INK_COLOR: u32 = 0xC81E3A;

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

/// The card lens: the clear preset's base look with the per-card
/// Fresnel knobs applied — the only source of row differences.
fn lens_card(gain: f32, f0: f32, power: f32) -> GlassEffect {
    let mut style = GlassPreset::clear().style();
    style.glass = style
        .glass
        .fresnel(gain)
        .fresnel_f0(f0)
        .fresnel_power(power);
    style.into_effect()
}

#[component]
fn Card(label: &'static str, material: Option<EffectStack>) -> impl IntoElement {
    let material = material.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={8.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                material={material}
                alignment="center"
                distribution="center"
            >
                <Text size={48.0} weight={700u16} color={Color::hex(INK_COLOR)}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

/// The static backdrop food: vertical saturated stripes, y-invariant.
/// Four stripes of 67 px make one 268 px period — exactly the card-pair
/// offset (card 220 + gap 48) — so both cards of a pair sample the same
/// content, and every row (same x) samples the same content too. The
/// stripe edges give the lens something to visibly bend; the saturation
/// gives the additive white glint something to read against.
fn stripes() -> Vec<Element> {
    const W: f32 = 67.0;
    const COLORS: [u32; 4] = [0xE5484D, 0xF5A524, 0x30A46C, 0x3B82F6];
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
                        label="FRESNEL OFF"
                        material=[lens_card(0.0, 0.0, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                    <Card
                        label="FRESNEL ON"
                        material=[lens_card(2.0, 0.0, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="GAIN 1.0"
                        material=[lens_card(1.0, 0.0, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                    <Card
                        label="GAIN 4.0"
                        material=[lens_card(4.0, 0.0, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="NO REFRACT"
                        material=[lens_card(2.0, 0.0, 0.0), Frost(), Rim(), EffectFresnel()]
                    />
                    <Card
                        label="WITH REFRACT"
                        material=[lens_card(2.0, 0.0, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="POWER 1.5"
                        material=[lens_card(1.0, 0.0, 1.5), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                    <Card
                        label="F0 0.6"
                        material=[lens_card(1.0, 0.6, 0.0), Refract(), Frost(), Rim(), EffectFresnel()]
                    />
                </Group>
            </Group>
        </Group>
    }
}
