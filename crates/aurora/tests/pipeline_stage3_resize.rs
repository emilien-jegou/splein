// Single responsibility: TDD verification for Stage 3 window resize damage and canvas preservation.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;

#[test]
fn test_3_9_window_grow_damages_exposed_strip_and_reports_preserved_rect() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(group().size(200.0, 200.0).fill(Color::BLACK));
    engine.frame();

    engine.resize(300, 200);
    let (_, _, _, diag) = engine.frame();

    assert_eq!(
        diag.spatial.preserved_canvas_rect,
        Some(ResolvedRect::new(0.0, 0.0, 200.0, 200.0)),
        "Scenario 3.9: Preserved canvas rect must be reported on resize"
    );
    let exposed_strip = ResolvedRect::new(200.0, 0.0, 100.0, 200.0);
    assert!(diag.spatial.damaged_rects.iter().any(|r| r.intersects(&exposed_strip)));
}
