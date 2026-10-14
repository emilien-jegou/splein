// Single responsibility: Showcase seamless retargeting of an in-flight spring.

use super::motion_ui::*;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Motion 03c · Seamless interruption";
pub const HOT_WIDTH: u32 = 780;
pub const HOT_HEIGHT: u32 = 440;
pub const HOT_BG: u32 = PAGE;

const RETARGET_SECS: f32 = 0.45;
const LEFT: f32 = -180.0;
const RIGHT: f32 = 180.0;
/// A spring retargeted faster than it can settle, so it is always mid-flight.
///
/// Nothing here coordinates the retargets. The interval flips a plain signal; the transition
/// adopts the live position and velocity on its own each time the target changes, which is
/// the whole claim this demo makes.
#[component]
pub fn Page() -> impl IntoElement {
    // A step counter, rather than a boolean, so the two endpoints stay distinguishable.
    let step = use_signal(0u32);
    let target = step.map(|n| if n % 2 == 0 { RIGHT } else { LEFT });
    use_interval(RETARGET_SECS, {
        let step = step;
        move || step.update(|n| *n += 1)
    });

    let x = Transition::from_str("spring").on(target);

    view! {
        <Shell
            eyebrow_label="MOTION · 03C — INTERRUPTION"
            title="Retarget mid-flight without a single pop."
            copy="A snappy spring needs roughly 600ms to settle, but it is retargeted every 450ms. Each retarget adopts the live position and velocity, so the card never jumps."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="CADENCE" value="retarget every 450ms" />}
                        right={<Stat label="CARRY" value="position + velocity" />}
                    />}
                >
                    <Card x={x} />
                </Panel>
            </Content>
        </Shell>
    }
}

#[component]
fn Card(x: Signal<f32>) -> impl IntoElement {
    view! {
        <Group
            key="card"
            direction="vertical"
            width={180.0}
            height="fill"
            radius={16.0}
            fill={ACCENT}
            shadow={Shadow::web_like(0.0, 10.0, 30.0, 0.0, Color::hex_alpha(ACCENT, 0.30))}
            alignment="center"
            distribution="center"
            gap={6.0}
            x={x}
        >
            <Text size={11.0} weight={700u16} color={Color::white_alpha(0.70)}>"SPRING"</Text>
            <Text size={18.0} weight={700u16} color={Color::WHITE}>"snappy"</Text>
            <Text size={12.0} color={Color::white_alpha(0.72)}>"retargeted 450ms"</Text>
        </Group>
    }
}
