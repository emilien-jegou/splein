// Single responsibility: Side-by-side web-parity demo for shadow styling.
//
// Mirrors a web reference 1:1 so the human can compare directly: a #F1CFFC
// background holding two 150x150 white cards (radius 8, no border) with
// centered labels above them, spaced as the reference (96v/48h breathing
// room, 48px between cards).
//   - "Typical Shadow": box-shadow rgba(0,0,0,0.35) 3px 6px 6px 0px,
//     via Shadow::web_like.
//   - "Optimized Shadow": five stacked tinted drop-shadows
//     rgba(122,61,143,0.15) at (0.5,1,1) (1,2,2) (2,4,4) (4,8,8) (8,16,16),
//     via Shadow::layered + Shadow::drop.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

#[path = "common/mod.rs"]
mod common;

use aurora::prelude::*;
use common::ExampleCli;

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 640;

const PANEL_BG: u32 = 0xF1CFFC;
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

fn build_ui() -> impl IntoElement {
    // The pink panel is the full background: no outer card, just the
    // two labeled cards centered with the reference's 96v/48h breathing room.
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
                margin="96.0 48.0"
                alignment="center"
                distribution="center"
            >
                <Card
                    label="Typical Shadow"
                    shadow={Shadow::web_like(3.0, 6.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35))}
                />
                // TEMPORARY diagnostics (remove after): zero offset isolates
                // whether the bottom band follows the offset; big offset
                // tests whether any hole scales with oy.
                <Card
                    label="probe zero-offset"
                    shadow={Shadow::web_like(0.0, 0.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35))}
                />
                <Card
                    label="probe big-offset"
                    shadow={Shadow::web_like(0.0, 20.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35))}
                />
                // TEMPORARY isolation (remove after): single layers tell
                // whether squareness lives in one layer (geometry) or only
                // in the stack (profile/stacking interaction).
                <Card
                    label="probe single big"
                    shadow={Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(8.0, 16.0, 16.0)}
                />
                <Card
                    label="probe single small"
                    shadow={Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(0.5, 1.0, 1.0)}
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
            </Group>
        </Group>
    }
}

fn main() {
    let cli = ExampleCli::parse(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Shadow Lab")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(PANEL_BG)
        .run(build_ui);
}
