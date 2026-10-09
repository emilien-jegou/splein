// Single responsibility: Playback page: a controllable ping-pong slide with hover and click.

use aurora::prelude::*;
use aurora::reactive::Signal;

use super::motion_ui::*;
use super::{SLIDE_SECS, TRAVEL_PX};

/// Builds the page shown for this concept.
///
/// This is the one shape a `transition.on(signal)` cannot express. A playback is not heading
/// anywhere: it has no target signal, because it is its own source of truth, and it needs
/// commands — pause, resume, restart, seek — that a value stream has nowhere to put. So it
/// yields a handle alongside the value, and the handlers below drive that handle directly.
#[component]
pub fn Page() -> impl IntoElement {
    let status = use_signal(String::from("playing"));

    // A self-driving ping-pong between both ends, with a controller for the handlers.
    let (slide, control) = use_playback(
        Transition::ease(SLIDE_SECS, Ease::InOutCubic).repeat(Repeat::PingPong),
        0.0,
        TRAVEL_PX,
    );

    view! {
        <Shell
            eyebrow_label="MOTION · 03D — PLAYBACK"
            title="Hover pauses. The pill restarts."
            copy="The card loops between both ends of the arena. Resting the pointer on it holds it still and releases it when you leave; the restart pill cancels the current slide and picks the far end."
        >
            <Content>
                <Panel head={<Strip
                    left={<Eyebrow label="CONTROLLER" />}
                    right={Controls(status.clone(), control.clone())}
                />}
                >
                    <Card control={control} slide={slide} />
                </Panel>
            </Content>
        </Shell>
    }
}

/// Status chip and restart pill sitting at the right of the card header.
#[component]
fn Controls(status: Signal<String>, control: ControllerHandle) -> impl IntoElement {
    view! {
        <Group direction="horizontal" gap={10.0} alignment="center">
            <StatusChip>
                <Text size={11.0} weight={700u16} color={Color::white_alpha(0.85)}>{status.clone()}</Text>
            </StatusChip>
            <RestartPill control={control} status={status} />
        </Group>
    }
}

/// Clickable restart target. It owns its interaction; nothing polls for clicks.
#[component]
fn RestartPill(control: ControllerHandle, status: Signal<String>) -> impl IntoElement {
    let handler = control.clone();
    let reporter = status.clone();
    view! {
        <Group
            key="restart"
            height={26.0}
            radius="max"
            fill={ACCENT}
            alignment="center"
            on:click={{
                move |_| {
                    handler.restart();
                    reporter.set(String::from("restarted"));
                }
            }}
        >
            <Text size={11.0} weight={700u16} color={Color::WHITE} margin={Margin::x(12.0)}>
                "Restart"
            </Text>
        </Group>
    }
}

/// The sliding card, with hover wired straight to the playback handle.
#[component]
fn Card(control: ControllerHandle, slide: Signal<f32>) -> impl IntoElement {
    let on_enter = control.clone();
    let on_leave = control.clone();
    let reporter = use_signal(String::from("playing"));
    let (say_paused, say_playing) = (reporter.clone(), reporter.clone());

    view! {
        <Group
            key="card"
            direction="vertical"
            width={140.0}
            height="fill"
            radius={16.0}
            fill={ACCENT}
            alignment="center"
            distribution="center"
            gap={6.0}
            x={slide}
            on:pointer-enter={{
                move |_| {
                    on_enter.pause();
                    say_paused.set(String::from("paused · hover"));
                }
            }}
            on:pointer-leave={{
                move |_| {
                    on_leave.resume();
                    say_playing.set(String::from("playing"));
                }
            }}
        >
            <StatusChip>
                <Text size={10.0} weight={700u16} color={Color::WHITE}>{reporter}</Text>
            </StatusChip>
            <Text size={11.0} weight={700u16} color={Color::white_alpha(0.70)}>"HOVER ME"</Text>
            <Text size={15.0} weight={700u16} color={Color::WHITE}>"pause on hover"</Text>
            <Text size={12.0} color={Color::white_alpha(0.72)}>"click Restart to replay"</Text>
        </Group>
    }
}
