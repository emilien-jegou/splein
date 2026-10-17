// Single responsibility: group blend-mode comparison — four identical
// panels over one identical split backdrop, differing only in the
// group's `blend=` mode. One variable per row: the blended group (fill,
// radius, label) is byte-identical everywhere; only how it composites
// with the backdrop behind it changes.
//
// Backdrop is a mid-light/mid-dark split (not white/black): Overlay and
// SoftLight preserve the backdrop's luminance and collapse to it at the
// extremes (at Cb=0.5 Overlay returns the source exactly; near 0/1 it
// vanishes), so mid-tones keep every mode visible against both a
// lighter and a darker surround in one frame — the prism_only lesson.
//
// Left column top → bottom: Normal (reference — the panel sits on the
// backdrop like paint), Multiply (darkens into the backdrop — orange
// over the light half goes brown, over the dark half near-black),
// Overlay (print overprint: punchier than both halves, contrast
// follows the backdrop), SoftLight (the gentlest — a tint, the halves
// read through). The panel geometry must render identically on all
// four — only the composite differs; that sameness is what the eyes
// check. Human judges; no test asserts.

use riode::internal::group;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never
/// mirrored.
pub const HOT_TITLE: &str = "Aurora — Group Blend";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 620;
pub const HOT_BG: u32 = 0x141619;

const MID_LIGHT: u32 = 0x9DA0A4;
const MID_DARK: u32 = 0x45484C;
/// The shared panel colour: saturated, mid-light — Multiply darkens it
/// visibly, Overlay/SoftLight keep its hue readable.
const ACCENT: u32 = 0xFF6A3D;
const HEAD: u32 = 0xE8E8EA;
const SUB: u32 = 0x8A8F96;
const LABEL: u32 = 0xB9BEC5;

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
                    blend={blend}
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

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" height="fill" fill={HOT_BG}>
            <Group
                direction="vertical"
                width="fill"
                height="fill"
                gap={14.0}
                margin="30.0 36.0"
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
            </Group>
        </Group>
    }
}
