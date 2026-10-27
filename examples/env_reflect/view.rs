// EnvReflect demo (A10): the backdrop mirror + sky at steep angles.
//
// A/B discipline (same instrument as the ggx/fresnel demos): every card
// of a pair sits over IDENTICAL backdrop content. The backdrop is a
// static field of vertical colour stripes whose period equals the
// card-pair offset (card 220 + gap 48 = 268 px), so the right card's
// food is the left card's food translated by exactly one period —
// pixel-for-pixel identical. The field is also y-invariant, so every
// row (same x) samples the same content too. Nothing drifts; the only
// difference within a pair is one EnvReflect knob or one stack item.
//
// ISOLATION: no card runs Rim, Fresnel, or GGX — the only edge light
// is the EnvReflect mirror composite itself. OFF is therefore the bare
// refracted card (lens + frost only): no brightening, no whitening.
// If you see rim light on an OFF card, that is a bug, not the baseline.
//
// The discriminating signature is the F-BLEND, not a glint: the mirror
// term REPLACES the transmitted image on the bevel
// (`col·(1-F) + env·F`), so the bevel DARKENS toward the mirror colour
// where the mirror takes over. An additive fake (Rim/Fresnel/GGX) can
// only brighten. The NO REFRACT card and the SKY card prove the two
// halves: no bevel ⇒ no mirror (flat top only), gentle slope ⇒ sky
// lift only.
//
// Rows (gain 1.0 — the F-blend needs no HDR gain; F≈0.04 on the flat
// face and F→1 only on the steepest bevel pixels, so the effect is a
// subtle tonal shift, not a glint):
//   - "ENV OFF" vs "ENV ON": everything else identical (Refract +
//     Frost only, then EnvReflect with gain 0 vs 1). ON must show the
//     bevel going slightly darker/cooler (the mirror of the dark food
//     outside the glass) plus a faint sky lift on the flat face. OFF
//     is the bare refracted card. Unmissable by construction: one card
//     has a bevel shadow the other lacks. (Whether it reads as a
//     "mirror" depends on the food — the stripes make the mirror read
//     as a tonal shift, not a picture.)
//   - "MIRROR vs SKY": same gain, same Refract — but NO FROST vs
//     frost: WITHOUT frost the mirror fetch reads the sharp food, so
//     the bevel mirror is crisp (you can see stripe colour flip across
//     the rim); WITH frost the mirror is soft. This row proves the
//     mirror samples the BACKDROP (frost changes it), not a constant.
//   - "NO REFRACT" vs "WITH REFRACT": no Refract ⇒ flat `n3=(0,0,1)` ⇒
//     R=(0,0,1) ⇒ sky everywhere, no mirror at all. The NO REFRACT
//     card must look like a uniform sky-tinted card — flat, no bevel
//     structure. WITH REFRACT restores the bevel mirror. The separation
//     proof.
//   - "SKY 0.3" vs "SKY 1.5": same gain, same stack — the flat-face
//     lift scales with sky strength. 0.3 is nearly invisible; 1.5 is a
//     clear milky lift on the face that never touches the bevel
//     mirror's structure. Proves the two terms (mirror vs sky) are
//     independent knobs.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{EffectStack, EnvReflect, Frost, GlassEffect, Refract};
use riode::internal::group;
use riode::prelude::*;

#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

pub const HOT_TITLE: &str = "Aurora — EnvReflect";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 1180;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0x2F6EBF;
const LABEL_COLOR: u32 = 0xFFFFFF;
const INK_COLOR: u32 = 0xC81E3A;

/// Gain baseline: the F-blend needs none of GGX's HDR gain — F does the
/// work (F≈0.04 on the face, →1 on the steepest bevel), so 1.0 lands a
/// subtle tonal shift. Every ON card uses the same gain; rows compare
/// exactly one knob.
const GAIN: f32 = 1.0;
/// Sky strength baseline: a mid grey that reads on dark food without
/// clipping (≈ the reference's uSky order of magnitude, pre-HDR).
const SKY: f32 = 0.6;

/// The card lens: the clear preset's base look with the per-card
/// EnvReflect knobs applied — the only source of row differences.
/// `gain` 0 = off; `sky` 0 keeps the shader default; `frost_off`
/// drops the Frost item (mirror reads sharp food).
fn lens_card(gain: f32, sky: f32) -> GlassEffect {
    let mut style = GlassPreset::clear_bare().style();
    style.glass = style.glass.env_reflect(gain);
    if sky > 0.0 {
        style.glass = style.glass.env_sky(sky);
    }
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

/// The static backdrop food: vertical saturated stripes, y-invariant,
/// period 268 px = card-pair offset. Darkened enough that the F-blend
/// darkening and the sky lift both read (mid-tone food clips neither).
fn stripes() -> Vec<Element> {
    const W: f32 = 67.0;
    const COLORS: [u32; 4] = [0x3A2424, 0x3D3416, 0x1E3329, 0x1E2A45];
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
                        label="ENV OFF"
                        material=[lens_card(0.0, 0.0), Refract(), Frost()]
                    />
                    <Card
                        label="ENV ON"
                        material=[lens_card(GAIN, SKY), Refract(), Frost(), EnvReflect()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="MIRROR CRISP"
                        material=[lens_card(GAIN, SKY), Refract(), EnvReflect()]
                    />
                    <Card
                        label="MIRROR SOFT"
                        material=[lens_card(GAIN, SKY), Refract(), Frost(), EnvReflect()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="NO REFRACT"
                        material=[lens_card(GAIN, SKY), Frost(), EnvReflect()]
                    />
                    <Card
                        label="WITH REFRACT"
                        material=[lens_card(GAIN, SKY), Refract(), Frost(), EnvReflect()]
                    />
                </Group>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="SKY 0.3"
                        material=[lens_card(GAIN, 0.3), Refract(), Frost(), EnvReflect()]
                    />
                    <Card
                        label="SKY 1.5"
                        material=[lens_card(GAIN, 1.5), Refract(), Frost(), EnvReflect()]
                    />
                </Group>
            </Group>
        </Group>
    }
}

fn page_bg() -> Fill {
    use riode::foundation::{ColorStop, LinearGradient, Point};
    Fill::LinearGradient(LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(0.0, 1180.0),
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Color::hex(0x2F6EBF),
            },
            ColorStop {
                position: 1.0,
                color: Color::hex(0x7B2FF7),
            },
        ],
    })
}
