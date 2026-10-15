// Single responsibility: Showcase static layout running once while a motion transform runs per frame.

use super::motion_ui::*;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Motion 01 · Layout vs compositor transform";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 440;
pub const HOT_BG: u32 = PAGE;

/// Radians per second of the sweep, matching what the page copy claims.
const SWEEP_HZ: f32 = 1.4;
/// Peak travel in logical pixels.
const TRAVEL: f32 = 120.0;
/// The page, written entirely as declarative reactive state.
///
/// The frame clock is the only source of motion, and it feeds a `.x()` binding. Because a
/// transform is a compositor override, nothing here can dirty layout: the card's box is
/// computed once and never recomputed, which is the entire point this demo makes.
#[component]
pub fn Page() -> impl IntoElement {
    let time = use_frame_clock();

    // A pure function of time. No driver, no engine handle, no per-frame closure.
    let travel = time.map(|t| ((t as f32) * SWEEP_HZ).sin() * TRAVEL);

    view! {
        <Shell
            eyebrow_label="MOTION · 01 — IDENTITY"
            title="Layout runs once. Transform runs every frame."
            copy="The card holds a fixed 280-wide box at a fixed origin. Only its compositor transform moves, so the layout boundary never re-executes and damage stays inside its old and new bounds."
        >
            <Content>
                <Group
                    direction="vertical"
                    width="fill"
                    height="fill"
                    radius={16.0}
                    fill={SURFACE}
                    stroke={Stroke::inside(1.0, hairline())}
                >
                    <Strip
                        left={<Stat label="LAYOUT" value="fixed box, computed once" />}
                        right={<Stat label="COMPOSITOR" value="translate(sin t) x 120px" />}
                    />
                    <Group width="fill" height={1.0} fill={hairline()} />
                    <Group width="fill" height="fill" margin={20.0}>
                        <Arena>
                            <Card travel={travel} />
                        </Arena>
                    </Group>
                </Group>
            </Content>
        </Shell>
    }
}

/// The one view that moves. Its box is fixed; only the transform binding changes.
#[component]
fn Card(travel: Computed<f32>) -> impl IntoElement {
    view! {
        <Group
            key="card"
            direction="vertical"
            width={280.0}
            height="fill"
            radius={14.0}
            fill={ACCENT}
            shadow={Shadow::web_like(0.0, 10.0, 30.0, 0.0, Color::hex_alpha(ACCENT, 0.35))}
            alignment="center"
            distribution="center"
            gap={6.0}
            x={travel}
        >
            <Text size={11.0} weight={700u16} color={Color::white_alpha(0.70)}>"STABLE VIEW"</Text>
            <Text size={17.0} weight={700u16} color={Color::WHITE}>"inspector-panel"</Text>
            <Text size={12.0} color={Color::white_alpha(0.72)}>"layout fixed · transform animated"</Text>
        </Group>
    }
}
