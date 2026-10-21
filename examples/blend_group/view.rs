// Single responsibility: group blend-mode comparison — identical panels
// over one identical split backdrop, differing only in the group's
// `blend=` mode. One variable per row: the blended group (fill, radius,
// label) is byte-identical everywhere; only how it composites with the
// backdrop behind it changes.
//
// Backdrop is a mid-light/mid-dark split (not white/black): Overlay and
// SoftLight preserve the backdrop's luminance and collapse to it at the
// extremes (at Cb=0.5 Overlay returns the source exactly; near 0/1 it
// vanishes), so mid-tones keep every mode visible against both a
// lighter and a darker surround in one frame — the prism_only lesson.
//
// Rows 1-4: solid panels at Normal / Multiply / Overlay / SoftLight.
// Glass rows: identical glass cards (same preset, same geometry) with
// the *card* blend — the whole glass result (refraction + frost + rim)
// composites against the sharp backdrop in-shader (Phase 3a). Normal
// is the exact pre-blend glass; Multiply/Overlay must show the card
// fusing into the split, the lens geometry unchanged.
//
// The panel/card geometry must render identically on every row of its
// section — only the composite differs; that sameness is what the eyes
// check. Human judges; no test asserts.

use riode::foundation::{BlendEffect, Effect, EffectStack, GlassEffect, GlassStyle, Glow, Refraction, Spectral};

fn blend_stack(blend: BlendMode) -> EffectStack {
    EffectStack::from(vec![BlendEffect::new(blend).boxed()])
}

fn glass_blend_stack(style: GlassStyle, blend: BlendMode) -> EffectStack {
    let GlassStyle { glass, backdrop, spectral, glow, refraction, .. } = style;
    EffectStack::from(vec![
        GlassEffect::single(glass, backdrop, spectral, glow, refraction).boxed(),
        BlendEffect::new(blend).boxed(),
    ])
}
use riode::internal::group;
use riode::prelude::*;
// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never
/// mirrored.
pub const HOT_TITLE: &str = "Aurora — Group Blend";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 1320;
pub const HOT_BG: u32 = 0x141619;

const MID_LIGHT: u32 = 0x9DA0A4;
const MID_DARK: u32 = 0x45484C;
/// The shared panel colour: saturated, mid-light — Multiply darkens it
/// visibly, Overlay/SoftLight keep its hue readable.
const ACCENT: u32 = 0xFF6A3D;
const HEAD: u32 = 0xE8E8EA;
const SUB: u32 = 0x8A8F96;
const LABEL: u32 = 0xB9BEC5;
const GLASS_INK: u32 = 0x1A1C1F;

/// One backdrop half, absolute (out of flow) so the panel centers in
/// the stage above both halves.
fn half(fill: u32, right: bool) -> Element {
    group()
        .width(Size::Percent(0.5))
        .height(Size::Percent(1.0))
        .absolute()
        .anchor(if right { Anchor::TopRight } else { Anchor::TopLeft })
        .fill(Color::hex(fill))
        .into_element()
}

#[component]
fn Row(label: &'static str, blend: BlendMode) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width="fill" height={104.0} gap={16.0} alignment="center">
            <Group width={112.0} height="fill" alignment="center">
                <Text size={13.0} weight={600u16} color={LABEL}>{label}</Text>
            </Group>
            <Group
                width="fill"
                height="fill"
                alignment="center"
                distribution="center"
            >
                {half(MID_LIGHT, false)}
                {half(MID_DARK, true)}
                // The one variable: `blend`. z-index above the halves
                // (they paint first, so the snapshot the blend reads
                // contains both).
                <Group
                    width={250.0}
                    height={72.0}
                    radius={16.0}
                    fill={ACCENT}
                    effects={blend_stack(blend)}
                    z_index={2}
                    alignment="center"
                    distribution="center"
                >
                    <Text size={18.0} weight={700u16} color={Color::WHITE}>{"BLEND"}</Text>
                </Group>
            </Group>
        </Group>
    }
}

/// Same split, same card geometry, same glass preset — only the card's
/// `blend=` differs. The card straddles the split edge so the lens
/// displacement is visible in both halves.
#[component]
fn GlassRow(label: &'static str, blend: BlendMode) -> impl IntoElement {
    let preset = GlassPreset::clear_bare();
    let style = GlassStyle {
        glass: preset.glass,
        backdrop: preset.backdrop,
        spectral: Spectral::NONE,
        glow: Glow::NONE,
        refraction: Refraction::NONE,
        shadow: None,
    };
    view! {
        <Group direction="horizontal" width="fill" height={120.0} gap={16.0} alignment="center">
            <Group width={112.0} height="fill" alignment="center">
                <Text size={13.0} weight={600u16} color={LABEL}>{label}</Text>
            </Group>
            <Group
                width="fill"
                height="fill"
                alignment="center"
                distribution="center"
            >
                {half(MID_LIGHT, false)}
                {half(MID_DARK, true)}
                <Group
                    width={220.0}
                    height={88.0}
                    radius={24.0}
                    effects={glass_blend_stack(style, blend)}
                    z_index={2}
                    alignment="center"
                    distribution="center"
                >
                    <Text size={16.0} weight={700u16} color={Color::hex(GLASS_INK)}>{"GLASS"}</Text>
                </Group>
            </Group>
        </Group>
    }
}

/// Same split, same card, same alpha-silhouette shadow — only the
/// *shadow's* `blend=` differs. `.alpha()` takes the EndLayer
/// silhouette path for both rows (the SDF direct path stays
/// approximated behind its warn); the card composite is plain
/// source-over in both — one variable: the shadow composite.
#[component]
fn ShadowRow(label: &'static str, blend: BlendMode) -> impl IntoElement {
    let shadow = Shadow::web_like(0.0, 10.0, 24.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.45))
        .alpha()
        .blend(blend);
    view! {
        <Group direction="horizontal" width="fill" height={120.0} gap={16.0} alignment="center">
            <Group width={112.0} height="fill" alignment="center">
                <Text size={13.0} weight={600u16} color={LABEL}>{label}</Text>
            </Group>
            <Group
                width="fill"
                height="fill"
                alignment="center"
                distribution="center"
            >
                {half(MID_LIGHT, false)}
                {half(MID_DARK, true)}
                <Group
                    width={220.0}
                    height={72.0}
                    radius={16.0}
                    fill={ACCENT}
                    shadow={shadow}
                    z_index={2}
                    alignment="center"
                    distribution="center"
                >
                    <Text size={18.0} weight={700u16} color={Color::WHITE}>{"SHADOW"}</Text>
                </Group>
            </Group>
        </Group>
    }
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" height="fill" fill={HOT_BG}>
            <Group
                direction="vertical"
                width="fill"
                height="fill"
                gap={12.0}
                margin="28.0 36.0"
                distribution="center"
            >
                <Text size={20.0} weight={650u16} color={HEAD}>{"Group blend modes"}</Text>
                <Text size={13.0} weight={400u16} color={SUB}>
                    {"identical panel, identical backdrop — only the blend differs"}
                </Text>
                <Row label="NORMAL" blend={BlendMode::Normal} />
                <Row label="MULTIPLY" blend={BlendMode::Multiply} />
                <Row label="OVERLAY" blend={BlendMode::Overlay} />
                <Row label="SOFT-LIGHT" blend={BlendMode::SoftLight} />
                <Text size={16.0} weight={650u16} color={HEAD}>{"Glass card blend"}</Text>
                <Text size={13.0} weight={400u16} color={SUB}>
                    {"whole glass result fuses into the backdrop — lens geometry unchanged"}
                </Text>
                <GlassRow label="GLASS" blend={BlendMode::Normal} />
                <GlassRow label="GLASS×MULT" blend={BlendMode::Multiply} />
                <GlassRow label="GLASS×OVER" blend={BlendMode::Overlay} />
                <Text size={16.0} weight={650u16} color={HEAD}>{"Shadow blend"}</Text>
                <Text size={13.0} weight={400u16} color={SUB}>
                    {"alpha-silhouette shadows — same silhouette path, only the shadow composite differs"}
                </Text>
                <ShadowRow label="SHADOW" blend={BlendMode::Normal} />
                <ShadowRow label="SHADOW×MULT" blend={BlendMode::Multiply} />
            </Group>
        </Group>
    }
}
