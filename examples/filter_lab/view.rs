// Single responsibility: Side-by-side demo for CSS element filters.
//
// Each card is a 150x150 rounded rect with a vivid fill, carrying one
// `filter` so the effect is isolated and comparable row-to-row:
//   - "Blur (8)": `Filter::blur(8)` — soft Gaussian falloff past the edges.
//   - "Brightness (1.6)": `Filter::brightness(1.6)` — lifts toward white.
//   - "Saturate (0)": `Filter::saturate(0)` — greyscale.
//   - "Contrast (1.8)": `Filter::contrast(1.8)` — deepens darks, blows lights.
//   - "Hue Rotate (120°)": `Filter::hue_rotate(120.0)` — spins the hue wheel.
//   - "Blur + Saturate": chained `blur(4).saturate(0)` — the spatial blur
//     runs before the color matrix (v1 ordering).
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Filter Lab";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = PANEL_BG;

pub(crate) const PANEL_BG: u32 = 0x1B1F2A;
const LABEL_COLOR: u32 = 0xC8D0E0;
/// Vivid base so greyscale / hue-rotate / brightness are unmistakable.
const CARD_FILL: u32 = 0x3B82F6;

#[component]
fn Card(label: &'static str, filter: Filter) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={24.0} alignment="center">
            <Text size={16.0} weight={500u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={150.0}
                height={150.0}
                radius={8.0}
                fill={CARD_FILL}
                filter={filter}
            />
        </Group>
    }
}

pub(crate) fn Page() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={PANEL_BG}
            alignment="center"
            distribution="center"
        >
            <Group
                direction="horizontal"
                width="fill"
                gap={40.0}
                margin="48.0 32.0"
                alignment="center"
                distribution="center"
            >
                <Card label="Blur (8)" filter={Filter::blur(8.0)} />
                <Card label="Brightness (1.6)" filter={Filter::brightness(1.6)} />
                <Card label="Saturate (0)" filter={Filter::saturate(0.0)} />
                <Card label="Contrast (1.8)" filter={Filter::contrast(1.8)} />
                <Card label="Hue Rotate (120°)" filter={Filter::hue_rotate(120.0)} />
                <Card label="Blur + Saturate" filter={Filter::blur(4.0).with(FilterOp::Saturate(0.0))} />
            </Group>
        </Group>
    }
}
