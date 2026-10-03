// Single responsibility: Temporary probe comparing shaped run extents against their own boxes.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;
use aurora::scene::SceneCommand;

fn report(width: u32, card_height: Size) -> Vec<String> {
    let mut engine = Engine::headless(width, 400);
    engine.mount(
        group().width(Size::Fill).height(Size::Fill).children((
            group()
                .width(Size::Fit)
                .height(card_height)
                .fill(Color::hex(0x334155))
                .children([text("reaches the edge of the box producing an ellipse").size(16.0)]),
            group()
                .width(Size::Fit)
                .height(card_height)
                .fill(Color::hex(0x0EA5E9))
                .children([text("04 TRACKING WEIGHTS ITALIC").size(16.0)]),
        )),
    );
    engine.frame();

    let mut out = Vec::new();
    let mut box_w = 0.0_f32;
    let mut box_h = 0.0_f32;
    for chunk in engine.scene().chunks.iter() {
        for cmd in chunk.commands.iter() {
            match cmd {
                SceneCommand::DrawRect { rect, .. } => {
                    box_w = rect.width;
                    box_h = rect.height;
                }
                SceneCommand::DrawText { layout, .. } => out.push(format!(
                    "box={:.1}x{:.1} run={:.1}x{:.1} lines={} h_over={:.1}",
                    box_w,
                    box_h,
                    layout.total_size.width,
                    layout.total_size.height,
                    layout.lines.len(),
                    layout.total_size.height - box_h,
                )),
                _ => {}
            }
        }
    }
    out
}

#[test]
fn probe_run_extents() {
    for width in [400u32, 160] {
        eprintln!("w={width:<5} FIT-h   {:?}", report(width, Size::Fit));
        eprintln!(
            "w={width:<5} FIX-h   {:?}",
            report(width, Size::Fixed(20.0))
        );
    }
}
