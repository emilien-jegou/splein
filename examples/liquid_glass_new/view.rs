// Single responsibility: Liquid-glass rebuild — the same Aurora page as
// `examples/liquid_glass`, but every glass card wears an inline
// `material={|geo| ...}` surface composed from DSL builders only.
//
// Pure optical distortion glass:
//   - All non-distortion decorative elements (washes, fake specular lobes,
//     and unaligned chromatic dispersion) removed.
//   - Uses Method 2's fitted lens decay (decay = 6.17 over band = 18.26px).
//   - Background blobs sweep directly across the cards to showcase
//     the 15px - 20px optical liquid refraction along the bevels.

#![allow(deprecated)]

use riode::foundation::{ColorStop, Fill, LinearGradient, Point};
use riode::internal::group;
use riode::prelude::*;

pub const HOT_TITLE: &str = "Aurora — liquid glass (pure distortion rebuild)";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = BG;

const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0x1C2333;

fn preset_shadow(radius: f32, opacity: f32) -> Shadow {
    Shadow::layered(Color::BLACK).ease_out((radius * 0.5).max(1.0), radius, opacity)
}

#[component]
pub fn Page() -> impl IntoElement {
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 8.0) / 8.0;
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
            // Animated background blobs sweeping directly behind the card rows
            {blob(
                t.clone(),
                150.0,
                Color::hex_alpha(0x3B82F6, 0.90),
                |v| Transform::from_translation(v * 750.0 + 80.0, 200.0), // Sweeps upper bevel
            )}
            {blob(
                t.clone(),
                120.0,
                Color::hex_alpha(0xE5484D, 0.85),
                |v| Transform::from_translation(850.0 - v * 750.0, 265.0), // Sweeps lower bevel
            )}
            {blob(
                t,
                100.0,
                Color::hex_alpha(0x30A46C, 0.85),
                |v| Transform::from_translation(v * 500.0 + 300.0, 230.0), // Sweeps card center
            )}

            <Group
                direction="vertical"
                gap={36.0}
                alignment="center"
                distribution="center"
                z_index={2}
            >
                // Row 1: Three primary glass cards demonstrating IOR progression
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <GlassCard
                        label="LENS · IOR 1.35"
                        caption="subtle liquid lens"
                        shadow={preset_shadow(20.0, 0.04)}
                        ior={1.35}
                    />
                    <GlassCard
                        label="LENS · IOR 1.50"
                        caption="fitted regular (Method 2)"
                        shadow={preset_shadow(20.0, 0.04)}
                        ior={1.50}
                    />
                    <GlassCard
                        label="LENS · IOR 1.65"
                        caption="dense flint glass"
                        shadow={preset_shadow(20.0, 0.04)}
                        ior={1.65}
                    />
                </Group>

                // Row 2: Capsule and small card shapes
                <Group direction="horizontal" gap={44.0} alignment="end">
                    <ShapeCaption caption="capsule · fitted lens">
                        <Group
                            width={240.0}
                            height={72.0}
                            radius="max"
                            material={|_geo| {
                                // Band scaled to the 36px capsule half-height
                                let n = Lens::beveled(1.0, 36.0);
                                Transmit::sample()
                                    .normal(n)
                                    .ior(1.50)
                                    .build()
                                    .into_surface()
                            }}
                            shadow={preset_shadow(20.0, 0.04)}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={14.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"CAPSULE"}
                            </Text>
                        </Group>
                    </ShapeCaption>

                    <ShapeCaption caption="small 56px lens">
                        <Group
                            width={56.0}
                            height={56.0}
                            radius={16.0}
                            material={|_geo| {
                                // Band scaled to 16px corner radius
                                let n = Lens::beveled(1.0, 16.0);
                                Transmit::sample()
                                    .normal(n)
                                    .ior(1.50)
                                    .build()
                                    .into_surface()
                            }}
                            shadow={preset_shadow(12.0, 0.03)}
                        />
                    </ShapeCaption>
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
    ior: f32,
) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                material={move |_geo| {
                    // Method 2 fitted lens: strength = 1.0, band = 18.2615px
                    let n = Lens::beveled(1.0, 18.2615);
                    Transmit::sample()
                        .normal(n)
                        .ior(ior)
                        .build()
                        .into_surface()
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
