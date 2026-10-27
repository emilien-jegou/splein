// Single responsibility: Liquid-glass rebuild — the same Aurora page as
// `examples/liquid_glass`, but every glass card wears a `material={...}`
// surface composed from DSL builders only. No `GlassPreset`, no preset
// module import: `Transmit` + `Specular` + `mix` + `fresnel`.
//
// Mapping (regular() → DSL, honest deltas in comments):
//   lens bend (band 18.26, decay 6.17, strength -2.6351)
//     → Normal::beveled(strength, band): a cos dome that is FLAT in the
//     interior, bending only inside `band` px of the edge — the preset
//     profile (edge-max displacement, flat face), not the raw edge_dist
//     gradient whose constant slope smeared the whole interior.
//     DELTA: dome peaks mid-band; the exact rim-pixel peak and the
//     mirrored-interior fold are not representable.
//   core frost σ 5.92 (Gaussian engine pre-blur) + wide tail
//     → Transmit frost quad-tap radius ≈ 1.5× σ (box taps spread less
//     energy than a Gaussian, so the tap radius runs hotter).
//   wash near-white 68% (a flat alpha MIX over the frosted sample)
//     → mix(transmit, Unlit wash colour, wash alpha). The old `.tint()`
//     was a linear MULTIPLY by ~white — a near-no-op that left no milky
//     body at all; wash must be a mix, staged BEFORE the specular lobe
//     (preset order: wash, then rim/specular on top).
//   dark wash 67% toward near-black → same mix (BOUNDED). The old Beer
//     `.attenuate(.., 0.55)` over an 80px path is exp(-100) ≈ pitch
//     black center — that was the "weird" dark card. Never again.
//   saturation 1.49 / tone LUT / ambient 0.18 → NO DSL NODE. Dropped
//     openly: refracted hues will read slightly flatter/greyer.
//   rim streak + isotropic white mix → Schlick-weighted GGX lobe via
//     mix(body, specular, fresnel(n, 1.5)), gain ≈ preset rim intensity
//     ballpark. DELTA: physical lobe vs preset streak fake.
//   glow/spectral/refraction NONE in regular → not needed. shadow kept
//     via the shared renderer path (same Shadow value the preset built).
//
// Human judges equivalence. Predictions are in the handoff, not claims.

use riode::foundation::{ColorStop, Fill, LinearGradient, Point};
use riode::internal::group;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`).
pub const HOT_TITLE: &str = "Aurora — liquid glass (material rebuild)";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0x1C2333;

/// Shared composer: frosted Snell transmit, flat wash mix toward
/// `wash_rgb` at `wash_w` (the preset wash alpha), then the GGX lobe
/// riding in on the Schlick weight (the preset rim/fresnel slot).
/// Returns the expression so tinted variants can wash further.
fn glass_body(
    band: f32,
    strength: f32,
    frost: f32,
    disperse: f32,
    wash_rgb: u32,
    wash_w: f32,
    spec_rough: f32,
    spec_gain: f32,
) -> MaterialExpr {
    let n = Lens::beveled(strength, band);
    let trans = Transmit::sample()
        .normal(n.clone())
        .ior(1.50)
        .frost(frost)
        .disperse(disperse)
        .build();
    let washed: MaterialExpr = mix(trans, Color::hex(wash_rgb).into(), wash_w);
    let lobe = Specular::ggx(n.clone(), spec_rough)
        .lit_by(spec_gain)
        .build();
    mix(washed, lobe, fresnel(n, 1.50))
}

/// regular(): wash 0xFEFDFD 68%, frost taps 9 (≈ σ 5.9).
fn mat_regular() -> Surface {
    glass_body(18.3, 8.0, 9.0, 0.0, 0xFEFDFD, 0.684, 0.08, 0.25).into_surface()
}

/// regular_dark(): wash 0x191818 67% (bounded mix — NOT Beer), frost 12
/// (≈ σ 8.2), slightly stronger lobe.
fn mat_regular_dark() -> Surface {
    glass_body(18.8, 8.0, 12.0, 0.0, 0x191818, 0.667, 0.10, 0.35).into_surface()
}

/// clear(): wash 0xFEFCFC 17%, frost 2 (≈ σ 1.3), tight lobe.
fn mat_clear() -> Surface {
    glass_body(18.4, 8.0, 2.0, 0.0, 0xFEFCFC, 0.17, 0.05, 0.25).into_surface()
}

/// clear_dark(): wash 0xFDFAFB 16%, frost 2 (≈ σ 1.2).
fn mat_clear_dark() -> Surface {
    glass_body(18.4, 8.0, 2.0, 0.0, 0xFDFAFB, 0.158, 0.05, 0.35).into_surface()
}

/// Preset shadow twin: what `lens()` builds — black layered, softness =
/// half the old sigma (≥1px), extent = radius, opacity = edge peak.
fn preset_shadow(radius: f32, opacity: f32) -> Shadow {
    Shadow::layered(Color::BLACK).ease_out((radius * 0.5).max(1.0), radius, opacity)
}

/// REGULAR capsule: pill radius, same lens band, wide frost.
fn mat_capsule() -> Surface {
    glass_body(18.3, 8.0, 9.0, 0.0, 0xFEFDFD, 0.684, 0.08, 0.25).into_surface()
}

/// Tinted: regular body, then the tint colour blended over at 0.36
/// (preset `style_tinted(blue 0.35)` × tint_strength 1.024).
fn mat_tinted() -> Surface {
    let base = glass_body(18.3, 8.0, 9.0, 0.0, 0xFEFDFD, 0.684, 0.08, 0.25);
    mix(base, Color::hex(0x3B82F6).into(), 0.36).into_surface()
}

/// Small-shape recipe: same wash, smaller frost (the preset shrinks core
/// blur under its size refs), same band.
fn mat_small() -> Surface {
    glass_body(18.3, 8.0, 7.0, 0.0, 0xFEFDFD, 0.684, 0.08, 0.25).into_surface()
}

/// Small tinted: small body + red tint 0.36.
fn mat_small_tinted() -> Surface {
    let base = glass_body(18.3, 8.0, 7.0, 0.0, 0xFEFDFD, 0.684, 0.08, 0.25);
    mix(base, Color::hex(0xE5484D).into(), 0.36).into_surface()
}

/// PRISM: regular recipe + chromatic fan (Spectral spread 0.5/reach 1.0
/// → 1px R/B taps at the band edges).
fn mat_prism() -> Surface {
    glass_body(18.3, 8.0, 9.0, 1.0, 0xFEFDFD, 0.684, 0.08, 0.25).into_surface()
}

#[component]
pub fn Page() -> impl IntoElement {
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 12.0) / 12.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });

    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Fill::Solid(Color::hex(BG))}
            clip={true}
            alignment="center"
            distribution="center"
            gap={40.0}
        >
            {blob(
                t.clone(),
                140.0,
                Color::hex_alpha(0x3B82F6, 0.85),
                |v| Transform::from_translation(v * 800.0 + 40.0, 60.0),
            )}
            {blob(
                t.clone(),
                110.0,
                Color::hex_alpha(0xE5484D, 0.8),
                |v| Transform::from_translation(900.0 - v * 800.0, 380.0),
            )}
            {blob(
                t,
                90.0,
                Color::hex_alpha(0x30A46C, 0.8),
                |v| Transform::from_translation(v * 420.0 + 280.0, 460.0 - v * 60.0),
            )}
            <Group
                direction="vertical"
                gap={36.0}
                alignment="center"
                distribution="center"
                z_index={2}
            >
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <GlassCard label="REGULAR" caption="wash 68% · frost 9" surface={mat_regular()} shadow={preset_shadow(20.8906, 0.0494)} />
                    <GlassCard label="REGULAR · DARK" caption="wash 67% · frost 12" surface={mat_regular_dark()} shadow={preset_shadow(20.3036, 0.0264)} />
                    <GlassCard label="CLEAR" caption="wash 17% · frost 2" surface={mat_clear()} shadow={preset_shadow(29.31, 0.0015)} />
                    <GlassCard label="CLEAR · DARK" caption="wash 16% · frost 2" surface={mat_clear_dark()} shadow={preset_shadow(13.0732, 0.0017)} />
                </Group>
                <Group direction="horizontal" gap={44.0} alignment="end">
                    <ShapeCaption caption="capsule · lens ellipse">
                        <Group
                            width={240.0}
                            height={72.0}
                            radius="max"
                            material={mat_capsule()}
                            shadow={preset_shadow(20.8906, 0.0494)}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={14.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"CAPSULE"}
                            </Text>
                        </Group>
                    </ShapeCaption>
                    <ShapeCaption caption="tinted · ambient colour kept">
                        <Group
                            width={150.0}
                            height={150.0}
                            radius={32.0}
                            material={mat_tinted()}
                            shadow={preset_shadow(20.8906, 0.0494)}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={13.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"TINTED"}
                            </Text>
                        </Group>
                    </ShapeCaption>
                    <Group direction="horizontal" gap={20.0} alignment="end">
                        <ShapeCaption caption="small · regular">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={mat_small()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · clear">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={mat_clear()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · dark">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={mat_regular_dark()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · tinted">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={mat_small_tinted()}
                            />
                        </ShapeCaption>
                    </Group>
                    <GlassCard
                        label="PRISM"
                        caption="disperse 1.0"
                        surface={mat_prism()}
                        shadow={preset_shadow(20.8906, 0.0494)}
                    />
                </Group>
            </Group>
        </Group>
    }
}

fn blob(
    t: Computed<f32>,
    size: f32,
    fill: Color,
    place: impl Fn(f32) -> Transform + 'static,
) -> Element {
    group()
        .width(size)
        .height(size)
        .radius("max")
        .absolute()
        .fill(fill)
        .transform(t.map(move |v| place(v)))
        .into_element()
}

#[component]
fn ShapeCaption(caption: &'static str, children: Vec<Element>) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            {children}
            <Text size={14.0} weight={700u16} color={Color::hex(LABEL_COLOR)}>{caption}</Text>
        </Group>
    }
}

#[component]
fn GlassCard(label: &'static str, caption: &'static str, surface: Surface, shadow: Shadow) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                material={surface}
                shadow={shadow}
                alignment="center"
                distribution="center"
            >
                <Text size={14.0} weight={700u16} color={Color::hex(INK_COLOR)}>{label}</Text>
            </Group>
            <Text size={11.0} weight={600u16} color={LABEL_COLOR}>{caption}</Text>
        </Group>
    }
}

/// Unused here (kept for layout parity with the reference demo's caption
/// helper set): the DSL cards print their wash/frost recipe directly.
#[allow(dead_code)]
fn _stripes() -> Fill {
    Fill::LinearGradient(LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(1100.0, 0.0),
        stops: vec![
            ColorStop { position: 0.0, color: Color::rgb(0.90, 0.90, 0.92) },
            ColorStop { position: 1.0, color: Color::rgb(0.90, 0.90, 0.92) },
        ],
    })
}
