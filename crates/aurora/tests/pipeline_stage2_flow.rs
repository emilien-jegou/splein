// Single responsibility: TDD verification for Stage 2 out-of-flow positioning and nested flow caching.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;

#[test]
fn test_2_10_absolute_element_does_not_affect_sibling_flow() {
    let mut engine = Engine::headless(600, 400);
    let abs_top = engine.signal(10.0f32);

    engine.mount(
        row().size(600.0, 400.0).children([
            group().size(100.0, 50.0).fill(Color::RED),
            group()
                .absolute()
                .margin_top(10.0)
                .size(50.0, 50.0)
                .fill(Color::BLUE),
            group().size(100.0, 50.0).fill(Color::GREEN),
        ]),
    );
    engine.frame();

    let (stats, _, _, diag) = engine.frame();
    assert_eq!(diag.compile.commands_emitted, 0);

    abs_top.set(50.0);
    let (stats, _, _, diag) = engine.frame();
    assert!(!stats.laid_out || diag.layout.cached_nodes.iter().any(|r| r.width == 100.0));
}

#[test]
fn test_2_11_box_in_boxed_re_render_keeps_outer_box_cached() {
    let mut engine = Engine::headless(800, 800);
    let inner_sig = engine.signal(50.0f32);

    engine.mount(
        group()
            .size(800.0, 800.0)
            .children([group().size(400.0, 400.0).children([group()
                .size(200.0, 200.0)
                .children([group().width(inner_sig.clone()).height(40.0)])])]),
    );
    engine.frame();

    inner_sig.set(80.0);
    let (stats, _, _, diag) = engine.frame();

    assert!(stats.laid_out);
    assert!(!diag
        .layout
        .recomputed_nodes
        .iter()
        .any(|r| r.width == 800.0));
    assert_eq!(diag.layout.recomputed_nodes.len(), 1);
    assert_eq!(diag.layout.recomputed_nodes[0].width, 200.0);
}
