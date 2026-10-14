// Single responsibility: Showcase press-and-release feedback on a clickable button.

use super::motion_ui::*;
use aurora::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Web 01 · Press feedback";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 440;
pub const HOT_BG: u32 = PAGE;

/// Target scale while the button is held.
const HELD: f32 = 0.9;
/// A button that squeezes on press and springs back on release.
///
/// The asymmetry is the point. A press is a short, definite event, so it eases down over
/// 90ms; a release is something the user watches for, so it springs back and overshoots.
/// Two transitions over one boolean express that, and the scale picks whichever is in
/// charge, with the idle one parked at rest.
#[component]
pub fn Page() -> impl IntoElement {
    let pressed = use_signal(false);
    let presses = use_signal(0u32);

    let down =
        Transition::from_str("out-cubic 90ms").on(pressed.map(
            |p| {
                if p {
                    HELD
                } else {
                    1.0
                }
            },
        ));
    let up =
        Transition::from_str("spring:bouncy")
            .on(pressed.map(|p| if p { 1.0 } else { 1.0 }));

    // Whichever stream owns the current state drives the view; the other sits at 1.0.
    // Plain closure: reads three signals, subscribes to all three, no .map()/.combine().
    let active = {
        let down = down;
        let up = up;
        derived!(move || if pressed.get() { down.get() } else { up.get() })
    };

    view! {
        <Shell
            eyebrow_label="WEB · 01 — PRESS FEEDBACK"
            title="Down in 90ms, back on a spring."
            copy="The press is a short eased squeeze; the release is a spring, so it overshoots slightly on the way home. Nothing requests frames while the button is at rest."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="GESTURE" value="pointer down / up" />}
                        right={<Stat label="RELEASE" value="Spring::bouncy" />}
                    />}
                >
                    <Group
                        direction="vertical"
                        width="fill"
                        height="fill"
                        alignment="center"
                        distribution="center"
                        gap={18.0}
                    >
                        <Button pressed={pressed} presses={presses} scale={active} />
                        <Text size={13.0} color={BODY}>
                            {presses.map(|n| format!("pressed {n} time{}", if n == 1 { "" } else { "s" }))}
                        </Text>
                        <Text size={12.0} color={MUTED}>"press and hold the button"</Text>
                    </Group>
                </Panel>
            </Content>
        </Shell>
    }
}

/// The button declares its own interaction, so nothing outside the tree has to know it exists.
///
/// Leaving the surface counts as a release, otherwise a pointer-down that slides off would
/// leave the button stuck in its pressed state.
#[component]
fn Button(pressed: Signal<bool>, presses: Signal<u32>, scale: Derived<f32>) -> impl IntoElement {
    let down = pressed;
    let up = pressed;
    let drop = pressed;
    let counted = presses;

    view! {
        <Group
            key="cta"
            height={52.0}
            radius="max"
            fill={ACCENT}
            alignment="center"
            distribution="center"
            scale={scale}
            on:pointer-down={{
                move |_| down.set(true)
            }}
            on:pointer-up={{
                move |_| up.set(false)
            }}
            on:pointer-leave={{
                move |_| drop.set(false)
            }}
            on:click={{
                move |info| counted.update(|n| *n += info.click_count as u32)
            }}
        >
            <Text size={15.0} weight={700u16} color={Color::WHITE} margin={Margin::x(28.0)}>
                "Continue"
            </Text>
        </Group>
    }
}
