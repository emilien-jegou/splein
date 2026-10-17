// Single responsibility: Liquid-glass material demo — the adaptive-glass
// variants (regular / clear) across light and dark appearance, a small-
// shape row, a capsule and a tinted shape, over a white field with moving
// colour blobs.
//
// The page is white so refracted colour and a spectral fringe read against
// a neutral field; drifting colour blobs are absolute children so the lens
// is unmistakable: the frost softens the blobs, the lens pulls a mirrored
// band of the interior in from the edge, the tone LUT and the rim ride on
// top. Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::foundation::{
    composed_blur_sigma, Backdrop, Glass, GlassStyle, Glow, Refraction, Spectral,
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
pub const HOT_TITLE: &str = "Aurora — Liquid glass";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = BG;

/// A neutral white field keeps refracted colour legible and stops the page
/// itself from supplying chroma; in-card text uses dark ink for the same
/// reason.
const BG: u32 = 0xFFFFFF;
const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0x1C2333;

#[component]
pub fn Page() -> impl IntoElement {
    // Slow ping-pong driver for the backdrop blobs: the glass sits still,
    // the world moves under it — refraction must follow.
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
                    <GlassCard label="REGULAR" preset={GlassPreset::regular()} />
                    <GlassCard label="REGULAR · DARK" preset={GlassPreset::regular_dark()} />
                    <GlassCard label="CLEAR" preset={GlassPreset::clear()} />
                    <GlassCard label="CLEAR · DARK" preset={GlassPreset::clear_dark()} />
                </Group>
                <Group direction="horizontal" gap={44.0} alignment="end">
                    <ShapeCaption caption="capsule · lens ellipse">
                        <Group
                            width={240.0}
                            height={72.0}
                            radius="max"
                            preset={GlassPreset::regular().style()}
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
                            preset={GlassPreset::regular().style_tinted(Color::hex_alpha(0x3B82F6, 0.35))}
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
                                preset={GlassPreset::regular().style()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · clear">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                preset={GlassPreset::clear().style()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · dark">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                preset={GlassPreset::regular_dark().style()}
                            />
                        </ShapeCaption>
                        <ShapeCaption caption="small · tinted">
                            <Group
                                width={56.0}
                                height={56.0}
                                radius={16.0}
                                preset={GlassPreset::regular().style_tinted(Color::hex_alpha(0xE5484D, 0.35))}
                            />
                        </ShapeCaption>
                    </Group>
                    <GlassCard
                        label="PRISM"
                        preset={GlassPreset::regular()}
                        spectral={Spectral::new().spread(0.5).reach(1.0)}
                    />
                </Group>
            </Group>
        </Group>
    }
}

/// One drifting colour blob. Absolute (out of flow — the glass row keeps
/// its centered layout) with the default absolute z (1); the cards
/// declare z=2 so they paint above while remaining in-flow.
///
/// Built with the fluent API: `absolute()`/`anchor()` are flag methods,
/// which RSX `absolute={true}` cannot call.
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

/// Caption under a bare shape, sized to its child (used by the capsule,
/// the tinted card and the small-shape row).
#[component]
fn ShapeCaption(caption: &'static str, children: Vec<Element>) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            {children}
            <Text size={11.0} weight={600u16} color={LABEL_COLOR}>{caption}</Text>
        </Group>
    }
}

/// Labelled glass card driven by a userland preset; the caption
/// reports the composed core sigma the pre-blur runs at (logical px,
/// from the card's own half shorter side) — read from the same
/// material the packer ships, so the caption can't drift from pixels.
#[component]
fn GlassCard(label: &'static str, preset: GlassPreset, tint: Option<Color>, spectral: Option<Spectral>) -> impl IntoElement {
    // Half the card's shorter side (220×160), logical px: what the
    // size-adaptive lens and frost scale themselves against.
    let backdrop = match tint {
        Some(t) => preset.backdrop.tint(t),
        None => preset.backdrop,
    };
    let caption = caption_for(&preset.glass, &backdrop, 80.0);
    let style = GlassStyle {
        glass: preset.glass,
        backdrop,
        spectral: spectral.unwrap_or(Spectral::NONE),
        glow: Glow::NONE,
        refraction: Refraction::NONE,
        shadow: Some(preset.shadow.clone()),
    };
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Group
                width={220.0}
                height={160.0}
                radius={24.0}
                preset={style}
                alignment="center"
                distribution="center"
            >
                <Text size={14.0} weight={700u16} color={Color::hex(INK_COLOR)}>{label}</Text>
            </Group>
            <Text size={11.0} weight={600u16} color={LABEL_COLOR}>{caption}</Text>
        </Group>
    }
}

/// `core sigma of the preset`, e.g. `core σ 5.9`.
fn caption_for(glass: &Glass, backdrop: &Backdrop, half_min: f32) -> String {
    let v = glass.effective(backdrop);
    format!("core σ {:.1}", composed_blur_sigma(v, half_min))
}
