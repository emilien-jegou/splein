// Single responsibility: Showcase transitions as reactive signal adapters over one boolean.

use super::motion_ui::*;
use aurora::prelude::*;

const TOGGLE_SECS: f32 = 1.4;
/// One signal driving four independent streams, two of them on different curves.
#[component]
pub fn Page() -> impl IntoElement {
    let enabled = use_interval_toggle(TOGGLE_SECS);

    // One declaration, three independent streams: the target says where, the
    // transition says how, and the frame loop supplies the time.
    // One declaration, three independent streams: the target says where, the
    // transition says how, and the frame loop supplies the time.
    let tween = Transition::from_str("out-cubic 200ms");
    let opacity = tween.on(enabled.clone().map(|on| if on { 1.0 } else { 0.35 }));
    let slide_x = tween.on(enabled.clone().map(|on| if on { 0.0 } else { 48.0 }));
    let spring = Transition::from_str("spring");
    let scale = spring.on(enabled.map(|on| if on { 1.18 } else { 1.0 }));

    view! {
        <Shell
            eyebrow_label="MOTION · 02 — SIGNAL TRANSITIONS"
            title="One signal. Three streams. No engine involvement."
            copy="Each property wraps its own target in a transition, so a tween and a spring can drive different properties of the same view off one boolean. Nothing animates implicitly: what is declared is exactly what moves."
        >
            <Content>
                <Group direction="horizontal" width="fill" height="fill" gap={20.0}>
                    <Panel
                        head={<Strip
                            left={<Eyebrow label="TWEEN" />}
                            right={text("Transition::from_str(\"out-cubic 200ms\")").size(12.0).color(BODY)}
                        />}
                    >
                        {tile("fade + slide", ACCENT).opacity(opacity).x(slide_x)}
                    </Panel>
                    <Panel
                        head={<Strip
                            left={<Eyebrow label="SPRING" />}
                            right={text("Transition::from_str(\"spring\")").size(12.0).color(BODY)}
                        />}
                    >
                        {tile("settle", 0x10B981).scale(scale)}
                    </Panel>
                </Group>
            </Content>
        </Shell>
    }
}

/// Accent tile used by both panels, sized so scaling never reaches the arena edge.
fn tile(label: &'static str, accent: u32) -> GroupDef {
    group()
        .direction("vertical")
        .width(Size::percent(0.62))
        .height("fill")
        .radius(14.0)
        .fill(Color::hex(accent))
        .alignment("center")
        .distribution("center")
        .gap(6.0)
        .children((
            text(label).size(16.0).weight(700).color(Color::WHITE),
            text("bound property")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.72)),
        ))
}
