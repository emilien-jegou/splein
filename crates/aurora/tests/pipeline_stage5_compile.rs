// Single responsibility: TDD verification for Stage 5 display list compilation, chunk caching, and culling.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;

#[test]
fn test_5_1_idle_frame_emits_zero_new_commands() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(group().width(400.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(Color::RED),
    ]));
    engine.frame(); // Warmup frame

    // Second consecutive frame with zero changes
    let (stats, _, _, diag) = engine.frame();

    assert!(!stats.laid_out, "Scenario 5.1: Idle frame does not lay out");
    assert_eq!(
        diag.compile.commands_emitted, 0,
        "Scenario 5.1: Invariant violated: Two frames in a row with no change must emit 0 commands"
    );
    assert_eq!(diag.spatial.damaged_rects.len(), 0);
}

#[test]
fn test_5_2_offscreen_nodes_emit_zero_commands_when_culled() {
    let mut engine = Engine::headless(400, 200);

    // Box 1 is visible (0..100); Box 2 is way offscreen (y=1000)
    engine.mount(group().direction(Direction::Vertical).width(400.0).height(200.0).children([
        group().width(100.0).height(100.0).fill(Color::RED),
        group().margin(Margin::top(900.0)).width(100.0).height(100.0).fill(Color::BLUE),
    ]));
    let (_, _, _, diag) = engine.frame();

    // Only the visible box should emit drawing commands
    assert!(diag.compile.commands_emitted <= 2, "Scenario 5.2: Offscreen node commands are culled");
}

#[test]
fn test_5_4_offscreen_node_with_shadow_reaching_onscreen_is_compiled() {
    let mut engine = Engine::headless(400, 200);

    // Box starts at y=210 (technically offscreen), but its shadow has offset_y = -30, reaching into view
    engine.mount(group().width(400.0).height(200.0).children([
        group()
            .margin(Margin::top(210.0))
            .width(100.0).height(50.0)
            .fill(Color::BLACK)
            .shadows([Shadow::outer(0.0, -30.0, 10.0, Color::BLACK)]),
    ]));
    let (_, _, _, diag) = engine.frame();

    assert!(
        diag.compile.commands_emitted > 0,
        "Scenario 5.4: Expanded shadow visual bounds must pull offscreen elements into compilation"
    );
}
