// Single responsibility: Verification test suite for Stage 1 invalidation, reactives, and boundaries.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::*;
use aurora::tree::*;

// =========================================================================
// 1.1 Reactive Fast-Path & Reconciliation Bypass Tests
// =========================================================================

#[test]
fn test_paint_mutation_fast_path_skips_layout() {
    let mut engine = Engine::headless(400, 400);
    let color = engine.signal(Color::BLACK);

    engine.mount(group().width(400.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(color.clone()),
    ]));

    engine.frame();

    // Mutate paint property: layout must be bypassed
    color.set(Color::WHITE);

    let FrameReport { stats, .. } = engine.frame();
    assert_eq!(stats.nodes_dirtied, 1);
    assert!(!stats.laid_out, "Paint-only changes must skip layout");
    assert!(stats.flags.contains(DirtyFlags::PAINT));
    assert!(!stats.flags.contains(DirtyFlags::LAYOUT));
}

#[test]
fn test_no_op_signal_writes_generate_zero_dirty_flags() {
    let mut engine = Engine::headless(400, 400);
    let width = engine.signal(100.0f32);

    engine.mount(group().width(400.0).height(400.0).children([
        group().width(width.clone()).height(50.0),
    ]));

    engine.frame();

    // Setting identical value must be suppressed
    width.set(100.0);

    let FrameReport { stats, .. } = engine.frame();
    assert_eq!(stats.nodes_dirtied, 0);
    assert!(!stats.laid_out);
    assert_eq!(stats.flags, DirtyFlags::NONE);
}

#[test]
fn test_static_elements_allocate_zero_subscribers() {
    let mut engine = Engine::headless(300, 200);

    engine.mount(group().width(300.0).height(200.0).children((
        group().width(60.0).height(30.0),
        group().width(60.0).height(30.0),
    )));

    engine.frame();

    let active_subs = engine.runtime().borrow_mut().drain_dirty_subscribers();
    assert!(active_subs.is_empty(), "Static elements must allocate 0 subscribers");
}

#[test]
fn test_dynamic_branching_resubscription_with_observer_guard() {
    let mut engine = Engine::headless(400, 200);
    let condition = engine.signal(false);
    let branch_a = engine.signal(50.0f32);
    let branch_b = engine.signal(150.0f32);

    let cond = condition.clone();
    let a = branch_a.clone();
    let b = branch_b.clone();

    let dynamic_width = computed(move || {
        if cond.get() {
            Size::Fixed(b.get())
        } else {
            Size::Fixed(a.get())
        }
    });

    engine.mount(group().width(400.0).height(200.0).children([
        group().width(dynamic_width).height(50.0),
    ]));

    engine.frame();

    // Flip to branch B
    condition.set(true);
    let FrameReport { stats: stats_flip, .. } = engine.frame();
    assert!(stats_flip.laid_out);

    // Branch B is now active: must trigger layout
    branch_b.set(180.0);
    let FrameReport { stats: stats_b, .. } = engine.frame();
    assert_eq!(stats_b.nodes_dirtied, 1);
    assert!(stats_b.laid_out);

    // Branch A is now dormant: must be ignored
    branch_a.set(999.0);
    let FrameReport { stats: stats_a, .. } = engine.frame();
    assert_eq!(stats_a.nodes_dirtied, 0);
    assert!(!stats_a.laid_out);
}

// =========================================================================
// 1.2 Orthogonal Invalidation & Bitflags
// =========================================================================

#[test]
fn test_paint_mutation_preserves_layout_caches() {
    let mut engine = Engine::headless(300, 200);
    let opacity = engine.signal(1.0f32);

    engine.mount(group().width(300.0).height(200.0).opacity(opacity.clone()).children([
        group().width(100.0).height(50.0),
    ]));

    engine.frame();

    opacity.set(0.5);
    let FrameReport { stats, .. } = engine.frame();
    assert!(!stats.laid_out);
    assert!(stats.flags.contains(DirtyFlags::PAINT));
}

#[test]
fn test_dirty_flags_orthogonal_operators() {
    let mut flags = DirtyFlags::NONE;
    assert!(flags.is_empty());

    flags.insert(DirtyFlags::PAINT);
    assert!(flags.contains(DirtyFlags::PAINT));
    assert!(!flags.contains(DirtyFlags::LAYOUT));

    let layout_flags = DirtyFlags::LAYOUT | DirtyFlags::MEASURE;
    assert!(layout_flags.contains(DirtyFlags::LAYOUT));
    assert!(layout_flags.contains(DirtyFlags::MEASURE));

    let masked = (flags | layout_flags) & !DirtyFlags::PAINT;
    assert!(!masked.contains(DirtyFlags::PAINT));
    assert!(masked.contains(DirtyFlags::LAYOUT));
}

// =========================================================================
// 1.3 Strict 2D Containment & Constraint Preservation
// =========================================================================

#[test]
fn test_strict_2d_containment_isolates_subtree_layout() {
    let mut engine = Engine::headless(800, 600);
    let child_width = engine.signal(50.0f32);

    engine.mount(group().width(800.0).height(600.0).children([
        group().width(200.0).height(100.0).children([
            group().width(child_width.clone()).height(30.0),
        ]),
        group().width(200.0).height(100.0),
    ]));

    engine.frame();

    child_width.set(80.0);

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
    assert_eq!(stats.nodes_dirtied, 1);
    assert!(stats.flags.contains(DirtyFlags::LAYOUT));
}

#[test]
fn test_dimension_mutation_escalates_to_parent_boundary() {
    let mut engine = Engine::headless(500, 500);
    let width = engine.signal(80.0f32);

    engine.mount(group().width(500.0).height(500.0).children([
        group().width(300.0).height(200.0).children([
            group().width(width.clone()).height(40.0),
            group().width(50.0).height(40.0),
        ]),
    ]));

    engine.frame();

    width.set(120.0);

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
    assert!(stats.flags.contains(DirtyFlags::LAYOUT));
}

// =========================================================================
// 1.4 Multi-Boundary Subtree Execution & Bulk Coalescence
// =========================================================================

#[test]
fn test_multiple_independent_boundaries_execute_without_root() {
    let mut engine = Engine::headless(1000, 1000);
    let w1 = engine.signal(40.0f32);
    let w2 = engine.signal(40.0f32);

    engine.mount(group().width(1000.0).height(1000.0).children([
        group().width(200.0).height(100.0).children([group().width(w1.clone()).height(20.0)]),
        group().width(200.0).height(100.0).children([group().width(w2.clone()).height(20.0)]),
    ]));

    engine.frame();

    w1.set(60.0);
    w2.set(70.0);

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
    assert_eq!(stats.nodes_dirtied, 2);
}

#[test]
fn test_bulk_update_coalesces_to_root_pass() {
    let mut engine = Engine::headless(800, 800);

    let signals: Vec<_> = (0..20)
        .map(|i| engine.signal(20.0f32 + i as f32))
        .collect();

    let cards: Vec<_> = signals
        .iter()
        .map(|s| group().width(100.0).height(40.0).children([group().width(s.clone()).height(20.0)]))
        .collect();

    engine.mount(group().width(800.0).height(800.0).children(cards));
    engine.frame();

    for (i, s) in signals.iter().enumerate() {
        s.set(50.0 + i as f32);
    }

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
    assert_eq!(stats.nodes_dirtied, 20);
}
