// Single responsibility: Slow-drift text repro for subpixel animation triage.
//
// Four runs drift at ~15px/s (subpixel per frame) to judge animation
// smoothness: X-only, Y-only and diagonal at 15px (hinted outlines), plus
// Y-only at 22px (unhinted, above the 16px hint cutoff). The rasterizer uses
// an axis-split phase policy — X renders at its nearest quarter-pixel bin,
// Y frozen at bin zero — with the fractional remainder blended explicitly
// and placed at integer pixels, so motion glides without morphing and rest
// frames stay crisp. Human judges shimmer; no test asserts.

use super::motion_ui::*;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Text smooth drift repro";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 620;
pub const HOT_BG: u32 = PAGE;

/// Travel in px per axis.
const TRAVEL: f32 = 120.0;
/// Three slow runs: horizontal, vertical, diagonal.
#[component]
pub fn Page() -> impl IntoElement {
    // Frame-clock ping-pong: moves from the first frame, no 12s dead wait.
    // 16s period (8s out, 8s back) at 120px => ~15px/s, subpixel per frame.
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 16.0) / 16.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });

    let x = t.clone().map(|v| v * TRAVEL);
    let y = t.clone().map(|v| v * TRAVEL);
    let xy = t
        .clone()
        .map(|v| Transform::from_translation(v * TRAVEL, v * TRAVEL));
    let y_big = t.map(|v| v * TRAVEL);

    view! {
        <Shell
            eyebrow_label="TEXT · SMOOTH DRIFT"
            title="Slow text repro."
            copy="Four runs drift at ~15px/s: X, Y and diagonal at 15px (hinted), Y at 22px (unhinted). Step a frame at a time (F8, then '.') — each press advances 1/8px, so every press should visibly blend, never jump. Rest frames stay pixel-crisp."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={16.0}>
                    <DriftRow
                        label="X · 15PX"
                        element={group().children([drift_text()]).x(x)}
                    />
                    <DriftRow
                        label="Y · 15PX"
                        element={group().children([drift_text()]).y(y)}
                    />
                    <DriftRow
                        label="X+Y · 15PX"
                        element={group().children([drift_text()]).transform(xy)}
                    />
                    <DriftRow
                        label="Y · 22PX"
                        element={group().children([drift_text_big()]).y(y_big)}
                    />
                </Group>
            </Content>
        </Shell>
    }
}

/// Fixed-height track holding one drifting run.
#[component]
fn DriftRow(label: &'static str, element: Element) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={64.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group
                width="fill"
                height="fill"
                radius={12.0}
                fill={SURFACE}
                stroke={Stroke::inside(1.0, hairline())}
                alignment="center"
            >
                {element}
            </Group>
        </Group>
    }
}

/// Shared sample copy so all three axes render identical glyphs.
fn drift_text() -> TextDef {
    text("Pack my box with five dozen liquor jugs 0123456789")
        .size(15.0)
        .weight(500)
        .color(INK)
}

/// Same copy above the hint cutoff (16px): unhinted linear outlines.
fn drift_text_big() -> TextDef {
    text("Pack my box with five dozen liquor jugs 0123456789")
        .size(22.0)
        .weight(500)
        .color(INK)
}
