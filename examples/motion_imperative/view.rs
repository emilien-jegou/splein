// Single responsibility: Showcase one target pose driving several independent property streams.

use super::motion_ui::*;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Motion 03 · Reactive targets";
pub const HOT_WIDTH: u32 = 820;
pub const HOT_HEIGHT: u32 = 460;
pub const HOT_BG: u32 = PAGE;

const STEP_SECS: f32 = 1.7;
const TRAVEL: f32 = 180.0;
/// Two views, one boolean, five independent streams.
///
/// The pose is declared as data rather than issued as a command, so each property gets to
/// choose its own curve. The panel's three properties share one tween; the badge deliberately
/// does not, and settles on a spring instead.
#[component]
pub fn Page() -> impl IntoElement {
    let outward = use_interval_toggle(STEP_SECS);

    // Panel: one tween, applied to each property independently.
    let panel_ease = Transition::from_str("out-quint 650ms");
    let x = panel_ease.on(outward.map(|out| if out { 0.0 } else { TRAVEL }));
    let opacity = panel_ease.on(outward.map(|out| if out { 1.0 } else { 0.4 }));
    let scale = panel_ease.on(outward.map(|out| if out { 1.0 } else { 0.9 }));

    // Badge: a spring, so it carries no duration at all and arrives on its own terms.
    let badge_scale =
        Transition::from_str("spring:bouncy").on(outward.map(|out| if out { 1.25 } else { 1.0 }));

    view! {
        <Shell
            eyebrow_label="MOTION · 03 — REACTIVE TARGETS"
            title="One target. Several streams."
            copy="A single tween drives x, opacity and scale together under one duration and curve. The badge settles on a spring instead, so it carries no fixed duration at all."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="TARGET" value="x · opacity · scale" />}
                        right={<Stat label="EASE" value="650ms · OutQuint" />}
                    />}
                >
                    <Group direction="vertical" width="fill" height="fill">
                        <PanelCard x={x} opacity={opacity} scale={scale} />
                        <Badge scale={badge_scale} />
                    </Group>
                </Panel>
            </Content>
        </Shell>
    }
}

#[component]
fn PanelCard(x: Signal<f32>, opacity: Signal<f32>, scale: Signal<f32>) -> impl IntoElement {
    view! {
        <Group
            key="panel"
            direction="vertical"
            width={280.0}
            height="fill"
            radius={14.0}
            fill={ACCENT}
            shadow={Shadow::web_like(0.0, 10.0, 30.0, 0.0, Color::hex_alpha(ACCENT, 0.30))}
            alignment="center"
            distribution="center"
            gap={6.0}
            x={x}
            opacity={opacity}
            scale={scale}
        >
            <Text size={11.0} weight={700u16} color={Color::white_alpha(0.70)}>"MULTI-PROPERTY"</Text>
            <Text size={17.0} weight={700u16} color={Color::WHITE}>"panel"</Text>
            <Text size={12.0} color={Color::white_alpha(0.72)}>"x · opacity · scale"</Text>
        </Group>
    }
}

#[component]
fn Badge(scale: Signal<f32>) -> impl IntoElement {
    view! {
        <Group
            key="badge"
            direction="vertical"
            width={96.0}
            height={96.0}
            radius="max"
            fill={0x10B981}
            anchor={Anchor::TopRight}
            margin={24.0}
            alignment="center"
            distribution="center"
            gap={4.0}
            scale={scale}
        >
            <Text size={10.0} weight={700u16} color={Color::white_alpha(0.75)}>"SPRING"</Text>
            <Text size={13.0} weight={700u16} color={Color::WHITE}>"bouncy"</Text>
        </Group>
    }
}
