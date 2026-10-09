// Single responsibility: TDD verification for Stage 3 window resize damage and canvas preservation.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::{Engine, FrameReport};

#[test]
#[ntest::timeout(5_000)]
fn test_3_9_window_grow_damages_exposed_strip_and_reports_preserved_rect() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(group().width(200.0).height(200.0).fill(Color::BLACK));
    engine.frame();

    engine.resize(300, 200);
    let FrameReport {
        diagnostics: diag, ..
    } = engine.frame();

    assert_eq!(
        diag.spatial.preserved_canvas_rect,
        Some(ResolvedRect::new(0.0, 0.0, 200.0, 200.0)),
        "Scenario 3.9: Preserved canvas rect must be reported on resize"
    );
    let exposed_strip = ResolvedRect::new(200.0, 0.0, 100.0, 200.0);
    assert!(diag
        .spatial
        .damaged_rects
        .iter()
        .any(|r| r.intersects(&exposed_strip)));
}

/// A resize reallocates the backend surface, so no old pixel is trustworthy:
/// the engine must damage the FULL new viewport, not just exposed strips.
/// (A stale nonzero swapchain age would otherwise leave the "preserved"
/// region unpainted — tear. Mirrors WaylandBackend::needs_full.)
#[test]
#[ntest::timeout(5_000)]
fn test_3_10_window_resize_damages_full_viewport() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(group().width(200.0).height(200.0).fill(Color::BLACK));
    engine.frame();

    engine.resize(300, 250);
    let FrameReport {
        diagnostics: diag, ..
    } = engine.frame();

    let full = ResolvedRect::new(0.0, 0.0, 300.0, 250.0);
    assert!(
        diag.spatial.damaged_rects.iter().any(|r| {
            r.x <= 0.0 && r.y <= 0.0 && r.right() >= 300.0 && r.bottom() >= 250.0
        }),
        "resize must damage the full new viewport {full:?}, got {:?}",
        diag.spatial.damaged_rects
    );
}
