// Single responsibility: Accordion panels, headers, and the reveal they animate.

use aurora::prelude::*;
use aurora::reactive::Readable;

use super::motion_ui::*;

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
    page(
        "WEB · 02 — ACCORDION",
        "One panel open at a time.",
        "Each item animates its own height from a spring, and the copy fades on the same clock. Because the height is a real layout value rather than an animated box, every sibling below is laid out at the live height and nothing has to be revealed by a clip.",
        content(panel(
            strip(
                stat("LAYOUT", "animated height per item"),
                stat("MOTION", "spring signal"),
            ),
            list(open),
        )),
    )
}

/// The four stacked items.
///
/// Nothing here declares motion. Each item animates its own height, and this container lays out
/// at whatever those heights currently are, so a sibling below is positioned by the live height
/// rather than by an animated box offset from a settled one.
fn list(open: Signal<Option<usize>>) -> GroupDef {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .gap(12.0)
        .children((
            item(0, "What is Aurora?", "A retained layout and compositor engine for Wayland overlays, with a reactive DSL on top.", open.clone()),
            item(1, "Why does motion not reflow?", "Transforms and opacity are compositor overrides, so they invalidate paint only and never reach the layout pass.", open.clone()),
            item(2, "Where does the motion live?", "In the component. A spring drives a signal, layout reads that signal, and the engine only lays out and paints what it is given.", open.clone()),
            item(3, "How do springs settle?", "A damped spring integrates toward its target and reports settled once position and velocity are both inside tolerance.", open),
        ))
}

/// One accordion row whose height is a pure function of the open index.
///
/// Its header owns the click, so nothing outside the tree has to know this item exists.
#[component]
fn item(
    index: usize,
    title: &'static str,
    copy: &'static str,
    open: Signal<Option<usize>>,
) -> impl IntoElement {
    let is_open = open.map(move |current| current == Some(index));

    // The height is animated here rather than by the engine, so it is a real layout value at
    // every frame. The copy below is laid out against it and clipped by it in the same pass,
    // which is why nothing needs a separate reveal.
    let height = Transition::from_str("spring:smooth").on(is_open.clone().map(move |open| {
        if open {
            OPEN
        } else {
            HEADER
        }
    }));

    group()
        .key(format!("panel-{index}"))
        .direction("vertical")
        .width("fill")
        .height(height)
        .radius(12.0)
        .clip(true)
        .fill(SURFACE)
        .stroke(Stroke::inside(1.0, hairline()))
        .children((
            header(index, title, is_open.clone(), open.clone()),
            panel_body(index, copy, is_open),
        ))
}

/// Clickable header row carrying the item key and its rotating chevron.
fn header(
    index: usize,
    title: &'static str,
    is_open: Derived<bool>,
    open: Signal<Option<usize>>,
) -> GroupDef {
    let toggle_open = open.clone();
    let toggle_state = is_open.clone();
    let degrees = is_open.map(move |open| if open { 180.0 } else { 0.0 });

    group()
        .key(format!("item-{index}"))
        .direction("horizontal")
        .width("fill")
        .height(HEADER)
        .alignment("center")
        .margin(Margin::x(20.0))
        .gap(12.0)
        .on_click(move |_| {
            toggle_open.set(if toggle_state.get() {
                None
            } else {
                Some(index)
            });
        })
        .children((
            text(title).size(14.0).weight(600).color(INK),
            group().width("fill"),
            chevron(degrees),
        ))
}

/// Revealed copy that fades with this item's own open state.
///
/// The fade is keyed to *this* item rather than to whether anything is open, so a closed
/// item cannot be left visible by a sibling opening.
///
/// The copy is selectable body text: press-drag highlights it natively. The header title
/// above deliberately stays plain — it owns the open/close click, and a selectable title
/// would turn every drifted press into a drag and swallow the toggle.
fn panel_body(index: usize, copy: &'static str, is_open: Derived<bool>) -> impl IntoElement {
    // The copy fades on the same clock as the box, so it never appears before the reveal.
    let reveal =
        Transition::from_str("out-cubic 180ms")
            .on(is_open.map(move |open| if open { 1.0 } else { 0.0 }));
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        // Centre on the main axis, which for a column is vertical. This is what gives the box's
        // travel something to reveal: the copy is uncovered from both edges at once instead of
        // sitting under the header with the whole lower half of the panel to fill.
        .distribution("center")
        .margin(Margin::sides(0.0, 20.0, BODY_PAD, 20.0))
        .opacity(reveal)
        .children([text(copy)
            .key(format!("copy-{index}"))
            .selectable(true)
            .size(13.0)
            .line_height(COPY_LINE)
            .color(BODY)
            .width("fill")])
}

/// Rotating chevron driven by the item's own animated angle.
fn chevron(degrees: impl Readable<f32> + 'static) -> impl IntoElement {
    group()
        .width(18.0)
        .height(18.0)
        .alignment("center")
        .distribution("center")
        .rotate_degrees_from(degrees)
        .children([text("^").size(13.0).weight(700).color(MUTED)])
}
