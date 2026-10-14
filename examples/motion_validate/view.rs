// Single responsibility: Showcase an action button that grows and pushes the field aside.

use super::motion_ui::*;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Web 03 · Growing action";
pub const HOT_WIDTH: u32 = 780;
pub const HOT_HEIGHT: u32 = 460;
pub const HOT_BG: u32 = PAGE;

/// Collapsed and expanded widths of the action button.
const COLLAPSED: f32 = 52.0;
const EXPANDED: f32 = 196.0;
/// A button that widens on click and pushes the field beside it rather than covering it.
///
/// The width is a plain bound layout value and the row declares the motion, so expanding
/// springs the button's render box toward its new flow box instead of re-laying out every
/// frame. The click lives on the button; nothing polls for it.
#[component]
pub fn Page() -> impl IntoElement {
    let expanded = use_signal(false);

    // The width is animated *here*, not by the engine, and the label reads the same animated
    // signal. That is the whole point: the box and its text are two views of one value, so they
    // cannot disagree. The previous version bound both to the raw `expanded` flag and let the
    // engine spring the box alone, so the label swapped to "Validate" on frame one while the box
    // was still 52px wide -- a long word laid out in a box that had not grown yet.
    let width = Transition::from_str("spring").on(expanded.map(|is_open| {
        if is_open {
            EXPANDED
        } else {
            COLLAPSED
        }
    }));

    view! {
        <Shell
            eyebrow_label="WEB · 03 — GROWING ACTION"
            title="The button pushes the field instead of covering it."
            copy="The width is a spring in the component, and the label reads that same animated signal. One value drives both the box and its text, so the word always fits the box it is drawn in."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="ROW" value="fit width" />}
                        right={<Stat label="MOTION" value="spring width signal" />}
                    />}
                >
                    <Body width={width} expanded={expanded} />
                </Panel>
            </Content>
        </Shell>
    }
}

#[component]
fn Body(width: Signal<f32>, expanded: Signal<bool>) -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            distribution="center"
            gap={20.0}
        >
            <Row width={width} expanded={expanded} />
            <Text size={12.0} color={MUTED}>"press the action to expand it"</Text>
        </Group>
    }
}

/// Fit-width row, so growth pushes content rather than squeezing it.
///
/// No motion declared here. The width arriving is already an animated signal, so the row simply
/// lays out at whatever value it currently holds, and every frame of the growth is a real layout
/// rather than an animated box hiding a static one.
#[component]
fn Row(width: Signal<f32>, expanded: Signal<bool>) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width="fit" height={52.0} gap={12.0} alignment="center">
            <Action width={width} expanded={expanded} />
            <Field />
        </Group>
    }
}

/// The action button, which owns its own toggle.
#[component]
fn Action(width: Signal<f32>, expanded: Signal<bool>) -> impl IntoElement {
    let state = expanded;
    // The label reads the animated width, not the target. Crossing the midpoint therefore
    // happens partway through the spring, exactly when the box is actually wide enough for the
    // longer word.
    let label = width;

    view! {
        <Group
            key="action"
            width={width}
            height="fill"
            radius="max"
            clip={true}
            fill={ACCENT}
            alignment="center"
            distribution="center"
            on:click={{
                move |_| state.update(|open| *open = !*open)
            }}
        >
            <Text size={14.0} weight={700u16} color={Color::WHITE}>
                {label.map(|w| {
                    if w > (COLLAPSED + EXPANDED) * 0.5 {
                        String::from("Validate")
                    } else {
                        String::from("Go")
                    }
                })}
            </Text>
        </Group>
    }
}

#[component]
fn Field() -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width={320.0}
            height="fill"
            radius="max"
            fill={0xF7F8FA}
            stroke={Stroke::inside(1.0, hairline())}
            alignment="center"
        >
            <Text size={14.0} color={MUTED} margin={Margin::x(16.0)}>"you@example.com"</Text>
        </Group>
    }
}
