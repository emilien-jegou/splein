// Single responsibility: Liquid-glass rebuild — the same Aurora page as
// `examples/liquid_glass`, but every glass card wears an inline
// `material={|geo| ...}` surface composed from DSL builders only.
//
// Tuned for high-fidelity optical distortion & 120Hz rendering:
//   - Chromatic dispersion enabled across all cards for spectral edge-fringing
//     as elements slide behind the bevel.
//   - CLEAR variants run frost=0.0 (3 texture taps, razor-sharp distortion).
//   - REGULAR variants run frost=6.0 (smooth blend without 5-tap ghosting).
//   - Specular lobes tuned to 0.04 - 0.05 roughness (Blender GlassBSDF polish).

#![allow(deprecated)]

use riode::foundation::{ColorStop, Fill, LinearGradient, Point};
use riode::internal::group;
use riode::prelude::*;

pub const HOT_TITLE: &str = "Aurora — liquid glass (material rebuild)";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0x1C2333;

/// Preset shadow twin: what `lens()` builds — black layered, softness =
/// half the old sigma (≥1px), extent = radius, opacity = edge peak.
fn preset_shadow(radius: f32, opacity: f32) -> Shadow {
    Shadow::layered(Color::BLACK).ease_out((radius * 0.5).max(1.0), radius, opacity)
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
            // Moving saturated background blobs — watch the chromatic dispersion
            // shear their edges as they pass behind the cards
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
                    <GlassCard
                        label="REGULAR"
                        caption="frost 6 · prism 1.5 · polish 0.05"
                        shadow={preset_shadow(20.8906, 0.0494)}
                        frost={6.0}
                        disperse={1.5}
                        wash_rgb={0xFEFDFD}
                        wash_w={0.62}
                        spec_rough={0.05}
                        spec_gain={0.45}
                    />
                    <GlassCard
                        label="REGULAR · DARK"
                        caption="frost 7 · prism 1.8 · polish 0.05"
                        shadow={preset_shadow(20.3036, 0.0264)}
                        frost={7.0}
                        disperse={1.8}
                        wash_rgb={0x191818}
                        wash_w={0.65}
                        spec_rough={0.05}
                        spec_gain={0.55}
                    />
                    <GlassCard
                        label="CLEAR"
                        caption="sharp 120Hz · prism 2.5"
                        shadow={preset_shadow(29.31, 0.0015)}
                        frost={0.0}
                        disperse={2.5}
                        wash_rgb={0xFEFCFC}
                        wash_w={0.15}
                        spec_rough={0.04}
                        spec_gain={0.50}
                    />
                    <GlassCard
                        label="CLEAR · DARK"
                        caption="sharp 120Hz · prism 2.5"
                        shadow={preset_shadow(13.0732, 0.0017)}
                        frost={0.0}
                        disperse={2.5}
                        wash_rgb={0xFDFAFB}
                        wash_w={0.15}
                        spec_rough={0.04}
                        spec_gain={0.55}
                    />
                </Group>
                <Group direction="horizontal" gap={44.0} alignment="end">
                    <ShapeCaption caption="capsule · lens ellipse">
                        <Group
                            width={240.0}
                            height={72.0}
                            radius="max"
                            material={|geo| {
                                let n = Lens::beveled(8.0, geo.radius());
                                let trans = Transmit::sample()
                                    .normal(n.clone())
                                    .ior(1.50)
                                    .frost(6.0)
                                    .disperse(2.0)
                                    .build();
                                let washed: MaterialExpr = mix(trans, Color::hex(0xFEFDFD).into(), 0.62);
                                let lobe = Specular::ggx(n.clone(), 0.05).lit_by(0.45).build();
                                mix(washed, lobe, fresnel(n, 1.50)).into_surface()
                            }}
                            shadow={preset_shadow(20.8906, 0.0494)}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={14.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"CAPSULE"}
                            </Text>
                        </Group>
                    </ShapeCaption>
                    <ShapeCaption caption="tinted · stained glass">
                        <Group
                            width={150.0}
                            height={150.0}
                            radius={32.0}
                            material={|geo| {
                                let n = Lens::beveled(8.0, geo.radius());
                                let trans = Transmit::sample()
                                    .normal(n.clone())
                                    .ior(1.50)
                                    .frost(5.0)
                                    .disperse(2.0)
                                    .build();
                                let washed: MaterialExpr = mix(trans, Color::hex(0xFEFDFD).into(), 0.50);
                                let base = mix(washed, Color::hex(0x3B82F6).into(), 0.38);
                                let lobe = Specular::ggx(n.clone(), 0.05).lit_by(0.50).build();
                                mix(base, lobe, fresnel(n, 1.50)).into_surface()
                            }}
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
                                material={|geo| {
                                    let n = Lens::beveled(8.0, geo.radius());
                                    let trans = Transmit::sample()
                                        .normal(n.clone())
                                        .ior(1.50)
                                        .frost(5.0)
                                        .disperse(1.5)
                                        .build();
                                    let washed: MaterialExpr = mix(trans, Color::hex(0xFEFDFD).into(), 0.62);
                                    let lobe = Specular::ggx(n.clone(), 0.05).lit_by(0.45).build();
                                    mix(washed, lobe, fresnel(n, 1.50)).into_surface()
                                }}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · clear">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={|geo| {
                                    let n = Lens::beveled(8.0, geo.radius());
                                    let trans = Transmit::sample()
                                        .normal(n.clone())
                                        .ior(1.50)
                                        .frost(0.0)
                                        .disperse(2.5)
                                        .build();
                                    let washed: MaterialExpr = mix(trans, Color::hex(0xFEFCFC).into(), 0.15);
                                    let lobe = Specular::ggx(n.clone(), 0.04).lit_by(0.50).build();
                                    mix(washed, lobe, fresnel(n, 1.50)).into_surface()
                                }}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · dark">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={|geo| {
                                    let n = Lens::beveled(8.0, geo.radius());
                                    let trans = Transmit::sample()
                                        .normal(n.clone())
                                        .ior(1.50)
                                        .frost(6.0)
                                        .disperse(1.8)
                                        .build();
                                    let washed: MaterialExpr = mix(trans, Color::hex(0x191818).into(), 0.65);
                                    let lobe = Specular::ggx(n.clone(), 0.05).lit_by(0.55).build();
                                    mix(washed, lobe, fresnel(n, 1.50)).into_surface()
                                }}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · tinted">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                material={|geo| {
                                    let n = Lens::beveled(8.0, geo.radius());
                                    let trans = Transmit::sample()
                                        .normal(n.clone())
                                        .ior(1.50)
                                        .frost(5.0)
                                        .disperse(1.5)
                                        .build();
                                    let washed: MaterialExpr = mix(trans, Color::hex(0xFEFDFD).into(), 0.50);
                                    let base = mix(washed, Color::hex(0xE5484D).into(), 0.38);
                                    let lobe = Specular::ggx(n.clone(), 0.05).lit_by(0.45).build();
                                    mix(base, lobe, fresnel(n, 1.50)).into_surface()
                                }}
                            />
                        </ShapeCaption>
                    </Group>
                    <GlassCard
                        label="PRISM"
                        caption="disperse 3.5 · optical split"
                        shadow={preset_shadow(20.8906, 0.0494)}
                        frost={2.0}
                        disperse={3.5}
                        wash_rgb={0xFEFDFD}
                        wash_w={0.35}
                        spec_rough={0.04}
                        spec_gain={0.55}
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
fn GlassCard(
    label: &'static str,
    caption: &'static str,
    shadow: Shadow,
    frost: f32,
    disperse: f32,
    wash_rgb: u32,
    wash_w: f32,
    spec_rough: f32,
    spec_gain: f32,
) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                material={move |geo| {
                    let n = Lens::beveled(8.0, geo.radius());
                    let trans = Transmit::sample()
                        .normal(n.clone())
                        .ior(1.50)
                        .frost(frost)
                        .disperse(disperse)
                        .build();
                    let washed: MaterialExpr = mix(trans, Color::hex(wash_rgb).into(), wash_w);
                    let lobe = Specular::ggx(n.clone(), spec_rough).lit_by(spec_gain).build();
                    mix(washed, lobe, fresnel(n, 1.50)).into_surface()
                }}
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
