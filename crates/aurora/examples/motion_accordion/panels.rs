// Single responsibility: Accordion panels, headers, and the reveal they animate.

use aurora::prelude::*;
use aurora::reactive::{IntoProp, Signal};

use super::motion_ui::*;

/// Height of an item's header row.
const HEADER: f32 = 56.0;
/// Height of an item once its panel is revealed.
const OPEN: f32 = 176.0;

/// The four stacked items; only this container declares the layout transition.
pub fn list(open: Signal<Option<usize>>) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .gap(12.0)
        .layout_transition(Transition::Spring(Spring::snappy()))
        .children((
            item(0, "What is Aurora?", "A retained layout and compositor engine for Wayland overlays, with a reactive DSL on top.", open.clone()),
            item(1, "Why does motion not reflow?", "Transforms and opacity are compositor overrides, so they invalidate paint only and never reach the layout pass.", open.clone()),
            item(2, "What drives a FLIP?", "Child rects are snapshotted before layout; anything whose rect moved springs back to its new box.", open.clone()),
            item(3, "How do springs settle?", "A damped spring integrates toward its target and reports settled once position and velocity are both inside tolerance.", open),
        ))
}

/// One accordion row whose height is a pure function of the open index.
fn item(index: usize, title: &'static str, copy: &'static str, open: Signal<Option<usize>>) -> GroupDef {
    group()
        .key(format!("panel-{index}"))
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(open.map(move |current| if current == Some(index) { OPEN } else { HEADER }))
        .radius(12.0)
        .clip(true)
        .fill(Color::hex(SURFACE))
        .stroke(Stroke::inside(1.0, hairline()))
        .children((header(index, title, open.clone()), panel_body(copy, open)))
}

/// Clickable header row carrying the item key and its rotating chevron.
fn header(index: usize, title: &'static str, open: Signal<Option<usize>>) -> GroupDef {
    group()
        .key(format!("item-{index}"))
        .direction(Direction::Horizontal)
        .width(Size::fill())
        .height(HEADER)
        .alignment(Alignment::Center)
        .margin(Margin::x(20.0))
        .gap(12.0)
        .children((
            text(title).size(14.0).weight(600).color(Color::hex(INK)),
            group().width(Size::fill()),
            chevron(open.map(move |current| {
                if current == Some(index) {
                    Transform::from_rotation_degrees(180.0)
                } else {
                    Transform::IDENTITY
                }
            })),
        ))
}

/// Revealed copy that fades with the same signal driving the height.
fn panel_body(copy: &'static str, open: Signal<Option<usize>>) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .margin(Margin::sides(0.0, 20.0, 20.0, 20.0))
        .opacity(open.map(|current| if current.is_some() { 1.0 } else { 0.0 }))
        .animate(Tween::new(240.ms(), Ease::OutCubic))
        .children([text(copy).size(13.0).line_height(20.0).color(Color::hex(BODY)).width(Size::fill())])
}

/// Rotating chevron driven by the item's own bound transform.
fn chevron(transform: impl IntoProp<Transform>) -> impl IntoElement {
    group()
        .width(18.0)
        .height(18.0)
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .transform(transform)
        .animate(Tween::new(240.ms(), Ease::OutCubic))
        .children([text("^").size(13.0).weight(700).color(Color::hex(MUTED))])
}
