// Single responsibility: Showcase one spring signal driving several properties as pure functions.

use super::motion_ui::*;
use aurora::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Motion 05 · Signal-driven motion";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 520;
pub const HOT_BG: u32 = PAGE;

/// Seconds between reversals of the driver signal.
const SWING_SECS: f32 = 1.5;
/// One spring, published as one signal, with everything else derived from it.
///
/// The simulation is not driven here: the transition owns it, the ticker advances it, and
/// this component only states the four things that read it. Each row is a pure function of
/// the same animated value, which is why they cannot drift apart.
#[component]
pub fn Page() -> impl IntoElement {
    let on = use_interval_toggle(SWING_SECS);
    let target = on.map(|is_on| if is_on { 1.0 } else { 0.0 });

    // The one simulation. Everything below is `p.map(..)`.
    let p = Transition::from_str("spring").on(target);

    view! {
        <Shell
            eyebrow_label="MOTION · 05 — REACTIVE"
            title="One spring, published as one signal."
            copy="A single spring simulation is published to a signal on every frame. Travel, path, scale and colour below are pure functions of it, so they stay in lockstep with no imperative call."
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
                    <Group
                        direction="vertical"
                        width="fill"
                        height="fill"
                        gap={16.0}
                        margin={24.0}
                    >
                        <Row label="TRAVEL" element={travel(p)} />
                        <Row label="PATH" element={path(p)} />
                        <Row label="SCALE" element={scale(p)} />
                        <Row label="COLOUR" element={swatch(p)} />
                    </Group>
                </Group>
            </Content>
        </Shell>
    }
}

/// Label and animated element sharing one fixed-height row.
#[component]
fn Row(label: &'static str, element: Element) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={40.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height={40.0} alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group width="fill" height="fill" alignment="center">
                {element}
            </Group>
        </Group>
    }
}

/// Accent puck sliding along a bounded track.
fn travel(p: Signal<f32>) -> Element {
    view! {
        <Group
            width="fill"
            height="fill"
            radius="max"
            clip={true}
            fill={Color::hex_alpha(HAIRLINE, 0.07)}
        >
            <Group
                width={120.0}
                height="fill"
                radius="max"
                fill={ACCENT}
                x={p.map(|v| v * 380.0)}
            />
        </Group>
    }
    .into_element()
}

/// Marker travelling a sine path across the row.
fn path(p: Signal<f32>) -> Element {
    view! {
        <Group width="fill" height="fill">
            <Group
                width={20.0}
                height={20.0}
                radius="max"
                fill={0xEC4899}
                transform={p.map(|v| Transform::from_translation(v * 460.0, (v * 6.28).sin() * 9.0))}
            />
        </Group>
    }
    .into_element()
}

/// Pill scaling with the same driver.
fn scale(p: Signal<f32>) -> Element {
    view! {
        <Group
            width={140.0}
            height="fill"
            radius="max"
            fill={0x10B981}
            alignment="center"
            distribution="center"
            scale={p.map(|v| 1.0 + v * 0.2)}
        >
            <Text size={12.0} weight={700u16} color={Color::WHITE}>"scale"</Text>
        </Group>
    }
    .into_element()
}

/// Swatch whose fill is sampled from the same position.
fn swatch(p: Signal<f32>) -> Element {
    view! {
        <Group
            width="fill"
            height="fill"
            radius="max"
            fill={p.map(|v| Color::hex_alpha(0xF59E0B, 0.25 + v * 0.75))}
        />
    }
    .into_element()
}
