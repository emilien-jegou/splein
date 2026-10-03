// Single responsibility: Showcase transitions as reactive signal adapters over one boolean.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 820;
pub const HEIGHT: u32 = 460;
const TOGGLE_SECS: f32 = 1.4;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 02 · Signal transitions")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// One signal driving four independent streams, two of them on different curves.
#[component]
pub fn Page() -> impl IntoElement {
    let enabled = use_interval_toggle(TOGGLE_SECS);

    // One declaration, three independent streams: the target says where, the
    // transition says how, and the frame loop supplies the time.
    // One declaration, three independent streams: the target says where, the
    // transition says how, and the frame loop supplies the time.
    let tween = Transition::ease(200.ms(), Ease::OutCubic);
    let opacity = tween.on(enabled.clone().map(|on| if on { 1.0 } else { 0.35 }));
    let slide_x = tween.on(enabled.clone().map(|on| if on { 0.0 } else { 48.0 }));
    let spring = Transition::spring(Spring::snappy());
    let scale = spring.on(enabled.map(|on| if on { 1.18 } else { 1.0 }));

    page(
        "MOTION · 02 — SIGNAL TRANSITIONS",
        "One signal. Three streams. No engine involvement.",
        "Each property wraps its own target in a transition, so a tween and a spring can drive different properties of the same view off one boolean. Nothing animates implicitly: what is declared is exactly what moves.",
        content(
            group()
                .direction(Direction::Horizontal)
                .width(Size::fill())
                .height(Size::fill())
                .gap(20.0)
                .children((
                    panel(
                        strip(
                            eyebrow("TWEEN"),
                            text("Transition::ease(200ms, OutCubic)")
                                .size(12.0)
                                .color(Color::hex(BODY)),
                        ),
                        tile("fade + slide", ACCENT).opacity(opacity).x(slide_x),
                    ),
                    panel(
                        strip(
                            eyebrow("SPRING"),
                            text("Transition::spring(Spring::snappy())")
                                .size(12.0)
                                .color(Color::hex(BODY)),
                        ),
                        tile("settle", 0x10B981).scale(scale),
                    ),
                )),
        ),
    )
}

/// Accent tile used by both panels, sized so scaling never reaches the arena edge.
fn tile(label: &'static str, accent: u32) -> GroupDef {
    group()
        .direction(Direction::Vertical)
        .width(Size::percent(0.62))
        .height(Size::fill())
        .radius(14.0)
        .fill(Color::hex(accent))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text(label).size(16.0).weight(700).color(Color::WHITE),
            text("bound property")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.72)),
        ))
}
