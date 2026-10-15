// Single responsibility: Accordion panels, headers, and the reveal they animate.

use riode::prelude::*;

use super::motion_ui::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Web 02 · Accordion";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 760;
pub const HOT_BG: u32 = PAGE;

/// Height of an item's header row.
pub const HEADER: f32 = 56.0;
/// Line height of the revealed copy, matching the size set on it below.
const COPY_LINE: f32 = 20.0;
/// Padding below the copy, matching the body's bottom margin.
const BODY_PAD: f32 = 20.0;
/// Height of an item's revealed body.
///
/// The copy is centred in a body taller than it needs, and that is deliberate. A body sized
/// exactly to the text would leave the dark arena around the list short by a card's worth, and
/// the arena is shared by every demo, so the slack belongs here rather than in the shell.
///
/// What matters is that the body is not *mostly* empty. The box springs from `HEADER` to
/// `OPEN` while the copy fades in, and every pixel of that travel has to reveal something --
/// otherwise the tail of the spring animates nothing, the copy sits fully opaque and untouched
/// while it happens, and the rounded bottom corners float below the text looking detached from
/// it. Centring the copy is what makes the whole travel meaningful: it is uncovered from both
/// edges at once, so the reveal reads as the panel opening rather than as a box growing.
/// Sized so the open list fits the height the page chrome leaves it, which is about 372px.
/// At 176 the four items wanted 380 and the layout shrink pass quietly squeezed every panel --
/// the open one to 173.07 and the closed ones to roughly 54 -- so the panels never had their
/// declared height and the reveal was measured against a box the layout had already shrunk.
///
/// This is the same defect the reorder list had, and it is invisible from a screenshot: the
/// panels look plausible while being a few pixels short. The number is bounded from above by the
/// space available, and `accordion_items_keep_their_declared_heights` in
/// `tests/layout_invalidation_regression.rs` fails the moment it stops fitting.
pub const OPEN: f32 = 168.0;

/// The page. Takes the open index as an argument rather than allocating it, so the same
/// component serves both the running demo and a test that drives the signal directly.
#[component]
pub fn Page(open: Signal<Option<usize>>) -> impl IntoElement {
    view! {
        <Shell
            eyebrow_label="WEB · 02 — ACCORDION"
            title="One panel open at a time."
            copy="Each item animates its own height from a spring, and the copy fades on the same clock. Because the height is a real layout value rather than an animated box, every sibling below is laid out at the live height and nothing has to be revealed by a clip."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="LAYOUT" value="animated height per item" />}
                        right={<Stat label="MOTION" value="spring signal" />}
                    />}
                >
                    <List open={open} />
                </Panel>
            </Content>
        </Shell>
    }
}

/// The four stacked items.
///
/// Nothing here declares motion. Each item animates its own height, and this container lays out
/// at whatever those heights currently are, so a sibling below is positioned by the live height
/// rather than by an animated box offset from a settled one.
#[component]
fn List(open: Signal<Option<usize>>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" height="fill" gap={12.0}>
            <Item
                index={0usize}
                title="What is Aurora?"
                copy="A retained layout and compositor engine for Wayland overlays, with a reactive DSL on top."
                open={open}
            />
            <Item
                index={1usize}
                title="Why does motion not reflow?"
                copy="Transforms and opacity are compositor overrides, so they invalidate paint only and never reach the layout pass."
                open={open}
            />
            <Item
                index={2usize}
                title="Where does the motion live?"
                copy="In the component. A spring drives a signal, layout reads that signal, and the engine only lays out and paints what it is given."
                open={open}
            />
            <Item
                index={3usize}
                title="How do springs settle?"
                copy="A damped spring integrates toward its target and reports settled once position and velocity are both inside tolerance."
                open={open}
            />
        </Group>
    }
}

/// One accordion row whose height is a pure function of the open index.
///
/// Its header owns the click, so nothing outside the tree has to know this item exists.
#[component]
fn Item(
    index: usize,
    title: &'static str,
    copy: &'static str,
    open: Signal<Option<usize>>,
) -> impl IntoElement {
    let is_open = open.map(move |current| current == Some(index));

    // The height is animated here rather than by the engine, so it is a real layout value at
    // every frame. The copy below is laid out against it and clipped by it in the same pass,
    // which is why nothing needs a separate reveal.
    let height = Transition::smooth().on(is_open.clone().map(move |open| {
        if open {
            OPEN
        } else {
            HEADER
        }
    }));

    view! {
        <Group
            key={format!("panel-{index}")}
            direction="vertical"
            width="fill"
            height={height}
            radius={12.0}
            clip={true}
            fill={SURFACE}
            stroke={Stroke::inside(1.0, hairline())}
        >
            <Header index={index} title={title} is_open={is_open.clone()} open={open} />
            <PanelBody index={index} copy={copy} is_open={is_open} />
        </Group>
    }
}

/// Clickable header row carrying the item key and its rotating chevron.
#[component]
fn Header(
    index: usize,
    title: &'static str,
    is_open: Computed<bool>,
    open: Signal<Option<usize>>,
) -> impl IntoElement {
    let toggle_open = open;
    let toggle_state = is_open.clone();
    let degrees = is_open.map(move |open| if open { 180.0 } else { 0.0 });

    view! {
        <Group
            key={format!("item-{index}")}
            direction="horizontal"
            width="fill"
            height={HEADER}
            alignment="center"
            margin={Margin::x(20.0)}
            gap={12.0}
            on:click={{
                move |_| {
                    toggle_open.set(if toggle_state.get() {
                        None
                    } else {
                        Some(index)
                    });
                }
            }}
        >
            <Text size={14.0} weight={600u16} color={INK}>{title}</Text>
            <Group width="fill" />
            <Chevron degrees={degrees} />
        </Group>
    }
}

/// Revealed copy that fades with this item's own open state.
///
/// The fade is keyed to *this* item rather than to whether anything is open, so a closed
/// item cannot be left visible by a sibling opening.
///
/// The copy is selectable body text: press-drag highlights it natively. The header title
/// above deliberately stays plain — it owns the open/close click, and a selectable title
/// would turn every drifted press into a drag and swallow the toggle.
#[component]
fn PanelBody(index: usize, copy: &'static str, is_open: Computed<bool>) -> impl IntoElement {
    // The copy fades on the same clock as the box, so it never appears before the reveal.
    let reveal =
        Transition::ease(180.ms(), Ease::OutCubic).on(is_open.map(
            move |open| {
                if open {
                    1.0
                } else {
                    0.0
                }
            },
        ));
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            // Centre on the main axis, which for a column is vertical. This is what gives the box's
            // travel something to reveal: the copy is uncovered from both edges at once instead of
            // sitting under the header with the whole lower half of the panel to fill.
            distribution="center"
            margin={Margin::sides(0.0, 20.0, BODY_PAD, 20.0)}
            opacity={reveal}
        >
            <Text
                key={format!("copy-{index}")}
                selectable={true}
                size={13.0}
                line_height={COPY_LINE}
                color={BODY}
                width="fill"
            >{copy}</Text>
        </Group>
    }
}

/// Rotating chevron driven by the item's own animated angle.
#[component]
fn Chevron(degrees: Computed<f32>) -> impl IntoElement {
    view! {
        <Group
            width={18.0}
            height={18.0}
            alignment="center"
            distribution="center"
            rotate_degrees_from={degrees}
        >
            <Text size={13.0} weight={700u16} color={MUTED}>"^"</Text>
        </Group>
    }
}
