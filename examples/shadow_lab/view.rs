// Single responsibility: Side-by-side web-parity demo for shadow styling.
//
// Mirrors a web reference 1:1 so the human can compare directly: a #F1CFFC
// background holding 150x150 cards (radius 8, no border) with centered
// labels above them, spaced as the reference (96v/48h breathing room,
// 48px between cards).
//   - "Typical Shadow": box-shadow rgba(0,0,0,0.35) 3px 6px 6px 0px,
//     via Shadow::web_like.
//   - "Optimized Shadow": five stacked tinted drop-shadows
//     rgba(122,61,143,0.15) at (0.5,1,1) (1,2,2) (2,4,4) (4,8,8) (8,16,16),
//     via Shadow::layered + Shadow::drop.
//   - "Inset Top": inset 0 4px 12px rgba(0,0,0,0.3) — darkens the top
//     interior (light from above); via Shadow::web_like(...).inset().
//   - "Inset Ring": inset 0 0 12px rgba(0,0,0,0.25) — even inner ring,
//     center stays clean.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Shadow Lab";
pub const HOT_WIDTH: u32 = 1280;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = PANEL_BG;

pub(crate) const PANEL_BG: u32 = 0xF1CFFC;
const LABEL_COLOR: u32 = 0x483050;

#[component]
fn Card(label: &'static str, shadow: Shadow) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={24.0} alignment="center">
            <Text size={16.0} weight={500u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={150.0}
                height={150.0}
                radius={8.0}
                fill={Color::WHITE}
                shadow={shadow}
            />
        </Group>
    }
}

/// Circle variant for the alpha-silhouette row: the content is a disc, so
/// a box fallback shows square corners while a true drop shadow follows
/// the circle — the two cannot look alike.
#[component]
fn RoundCard(label: &'static str, shadow: Shadow) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={24.0} alignment="center">
            <Text size={16.0} weight={500u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={150.0}
                height={150.0}
                radius={75.0}
                fill={Color::WHITE}
                shadow={shadow}
            />
        </Group>
    }
}

pub(crate) fn Page() -> impl IntoElement {
    // The pink panel is the full background: no outer card, just the
    // labeled cards centered with the reference's 96v/48h breathing room.
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
                gap={48.0}
                margin="48.0 48.0"
                alignment="center"
                distribution="center"
            >
                <Card
                    label="Typical Shadow"
                    shadow={Shadow::web_like(3.0, 6.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35))}
                />
                <Card
                    label="Optimized Shadow"
                    shadow={Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(8.0, 16.0, 16.0)}
                />
                <Card
                    label="Inset Top"
                    shadow={Shadow::web_like(0.0, 4.0, 12.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.3)).inset()}
                />
                <Card
                    label="Inset Ring"
                    shadow={Shadow::web_like(0.0, 0.0, 12.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.25)).inset()}
                />
                <RoundCard
                    label="Drop Shadow"
                    shadow={Shadow::web_like(0.0, 8.0, 16.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.4)).alpha()}
                />
            </Group>
        </Group>
    }
}
