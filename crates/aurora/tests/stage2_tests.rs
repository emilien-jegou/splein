// Test Suite: Stage 2 - Damage Tracking, Persistent Canvas & Swapchain Age
// Verifies:
// 2.1 Geometry Commit at Paint Time & Visual Bounds Expansion (Strokes/Shadows/Clips)
// 2.2 Removal Damage & Moved-Node Invalidation
// 2.3 Physical Clamping & Greedy Least-Added-Area Merging
// 2.4 Persistent Canvas Rasterization & Pixel Retention
// 2.5 Overlap Invalidation & Hierarchical Subtree Culling

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::*;
use aurora::tree::*;

// =========================================================================
// 2.3 DamageRegion & Outward Physical Rounding Math
// =========================================================================
#[test]
fn test_resolved_rect_math_and_physical_outward_rounding() {
    let r1 = ResolvedRect::new(10.25, 20.75, 100.25, 50.5);
    assert_eq!(r1.right(), 110.5);
    assert_eq!(r1.bottom(), 71.25);

    // Outward physical rounding at 1.0x scale
    let phys_1x = r1.to_physical_outward(1.0);
    assert_eq!(phys_1x.x, 10.0);
    assert_eq!(phys_1x.y, 20.0);
    assert_eq!(phys_1x.right(), 111.0);
    assert_eq!(phys_1x.bottom(), 72.0);

    // Outward physical rounding at 2.0x scale
    let phys_2x = r1.to_physical_outward(2.0);
    assert_eq!(phys_2x.x, 20.0);
    assert_eq!(phys_2x.y, 41.0);
    assert_eq!(phys_2x.right(), 221.0);
    assert_eq!(phys_2x.bottom(), 143.0);

    // Intersection
    let r2 = ResolvedRect::new(50.0, 40.0, 100.0, 100.0);
    let inter = r1.intersect(&r2);
    assert_eq!(inter.x, 50.0);
    assert_eq!(inter.y, 40.0);
    assert_eq!(inter.right(), 110.5);
    assert_eq!(inter.bottom(), 71.25);

    // Disjoint intersection
    let r3 = ResolvedRect::new(300.0, 300.0, 50.0, 50.0);
    assert!(r1.intersect(&r3).is_empty());
}

#[test]
fn test_damage_region_absorbs_overlapping_rects() {
    let mut damage = DamageRegion::new();

    // Push two overlapping rectangles
    damage.push(ResolvedRect::new(10.0, 10.0, 50.0, 50.0));
    damage.push(ResolvedRect::new(30.0, 30.0, 50.0, 50.0));

    // Overlapping rectangles must be automatically merged into their union
    assert_eq!(
        damage.rects().len(), 1,
        "Overlapping damage rects must be merged immediately"
    );
    let u = damage.rects()[0];
    assert_eq!(u.x, 10.0);
    assert_eq!(u.y, 10.0);
    assert_eq!(u.right(), 80.0);
    assert_eq!(u.bottom(), 80.0);
}

#[test]
fn test_damage_region_greedy_clustering_clamps_to_max_rects() {
    let mut damage = DamageRegion::new();

    // Push 4 strictly disjoint rectangles (fills capacity)
    damage.push(ResolvedRect::new(10.0, 10.0, 20.0, 20.0));
    damage.push(ResolvedRect::new(100.0, 10.0, 20.0, 20.0));
    damage.push(ResolvedRect::new(200.0, 10.0, 20.0, 20.0));
    damage.push(ResolvedRect::new(300.0, 10.0, 20.0, 20.0));
    assert_eq!(damage.rects().len(), 4);

    // Push a 5th disjoint rectangle close to the first rect:
    // Distance between (10, 10) and (35, 10) is minimal, so they must be merged
    damage.push(ResolvedRect::new(35.0, 10.0, 20.0, 20.0));

    assert_eq!(
        damage.rects().len(),
        DamageRegion::MAX_RECTS,
        "DamageRegion must strictly never exceed MAX_RECTS (4)"
    );

    // Verify the closest pair was clustered
    let bounding = damage.bounding_box();
    assert_eq!(bounding.x, 10.0);
    assert_eq!(bounding.right(), 320.0);
}

// =========================================================================
// 2.4 DamageRing & Swapchain Age Synchronization
// =========================================================================

#[test]
fn test_damage_ring_history_and_age_synchronization() {
    let mut ring = DamageRing::new();
    let full_window = ResolvedRect::new(0.0, 0.0, 800.0, 600.0);

    // Frame 1
    let d1 = ring.advance();
    d1.push(ResolvedRect::new(10.0, 10.0, 20.0, 20.0));

    // Frame 2
    let d2 = ring.advance();
    d2.push(ResolvedRect::new(50.0, 50.0, 20.0, 20.0));

    // Frame 3
    let d3 = ring.advance();
    d3.push(ResolvedRect::new(100.0, 100.0, 20.0, 20.0));

    // Age 0 must always force full window repaint
    let age0 = ring.damage_for_age(0, full_window);
    assert_eq!(age0.rects()[0], full_window);

    // Age 1: current frame only (Frame 3)
    let age1 = ring.damage_for_age(1, full_window);
    assert_eq!(age1.rects().len(), 1);
    assert_eq!(age1.rects()[0], ResolvedRect::new(100.0, 100.0, 20.0, 20.0));

    // Age 2: union of Frame 3 and Frame 2
    let age2 = ring.damage_for_age(2, full_window);
    assert_eq!(age2.rects().len(), 2);
    assert!(age2.rects().iter().any(|r| r.x == 100.0));
    assert!(age2.rects().iter().any(|r| r.x == 50.0));

    // Age 3: union of Frame 3, Frame 2, and Frame 1
    let age3 = ring.damage_for_age(3, full_window);
    assert_eq!(age3.rects().len(), 3);
    assert!(age3.rects().iter().any(|r| r.x == 100.0));
    assert!(age3.rects().iter().any(|r| r.x == 50.0));
    assert!(age3.rects().iter().any(|r| r.x == 10.0));
}

// =========================================================================
// 2.1 Visual Bounds Expansion (Strokes, Shadows, and Clips)
// =========================================================================

#[test]
fn test_visual_bounds_expansion_for_strokes_and_outer_shadows() {
    let mut node = LayoutNode::new(NodeKind::Group);
    node.resolved_rect = ResolvedRect::new(0.0, 0.0, 100.0, 50.0);

    // Add outside stroke of 4px
    node.style.appearance.stroke = Some(Stroke::outside(4.0, Color::BLACK));

    // Add outer drop shadow with blur=3.0, spread=2.0 (kernel pad = 2 + 3*3 = 11px)
    node.style.appearance.shadows = vec![
        Shadow::outer(0.0, 5.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5)),
    ];

    let abs_pos = Point::new(100.0, 200.0);
    let tx = Transform::from_translation(abs_pos.x, abs_pos.y);
    let bounds = node.compute_visual_bounds(&tx, None);

    // Visual bounds must expand past the base 100x50 rect
    assert!(bounds.x < 100.0, "Bounds must expand to the left for shadow pad");
    assert!(bounds.y < 200.0, "Bounds must expand upwards for shadow pad");
    assert!(bounds.right() > 200.0, "Bounds must expand to the right");
    assert!(bounds.bottom() > 255.0, "Bounds must expand downwards for shadow offset + pad");

    // When clamped by an ancestor clip boundary:
    let clip_box = ResolvedRect::new(100.0, 200.0, 100.0, 50.0);
    let clamped_bounds = node.compute_visual_bounds(&tx, Some(clip_box));
    assert_eq!(
        clamped_bounds, clip_box,
        "Ancestor clip must strictly clamp visual bounds expansion"
    );
}

// =========================================================================
// 2.2 Removal Damage & Moved-Node Invalidation
// =========================================================================

#[test]
fn test_removal_damage_captures_unmounted_node_bounds() {
    let mut engine = Engine::headless(500, 500);

    // Mount initial tree with child box at (50, 50, 80, 80)
    engine.mount(group().width(500.0).height(500.0).children([
        group().margin(Margin::sides(50.0, 50.0, 50.0, 50.0)).width(80.0).height(80.0).fill(Color::RED),
    ]));
    engine.frame();

    // Re-mount empty container (child is purged)
    engine.mount(group().width(500.0).height(500.0));
    engine.frame();

    let damage = engine.current_damage();
    assert!(
        !damage.is_empty(),
        "Purging a node must record its previous on-screen bounds as damage"
    );

    // The damaged area must intersect the unmounted child's previous screen coordinates
    let unmounted_child_bounds = ResolvedRect::new(50.0, 50.0, 80.0, 80.0);
    assert!(
        damage.rects().iter().any(|r| r.intersects(&unmounted_child_bounds)),
        "Removal damage must intersect the purged child's former position"
    );
}

#[test]
fn test_moved_node_triggers_dual_damage() {
    let mut engine = Engine::headless(800, 600);
    let width_sig = engine.signal(100.0f32);

    // Row containing Box A and Box B
    // When Box A widens, Box B moves to the right
    engine.mount(group().direction(Direction::Horizontal).width(800.0).height(600.0).children([
        group().width(width_sig.clone()).height(50.0).fill(Color::RED),
        group().width(80.0).height(50.0).fill(Color::BLACK), // Box B
    ]));
    engine.frame();

    // Box A widens from 100 to 200, shifting Box B from x=100 to x=200
    width_sig.set(200.0);
    engine.frame();

    let damage = engine.current_damage();
    // Damage must cover Box B's old position (x=100) AND its new position (x=200)
    let old_b_pos = ResolvedRect::new(100.0, 0.0, 80.0, 50.0);
    let new_b_pos = ResolvedRect::new(200.0, 0.0, 80.0, 50.0);

    assert!(
        damage.rects().iter().any(|r| r.intersects(&old_b_pos)),
        "Damage must cover moved node's previous position"
    );
    assert!(
        damage.rects().iter().any(|r| r.intersects(&new_b_pos)),
        "Damage must cover moved node's new position"
    );
}

// =========================================================================
// 2.4 Persistent Canvas Rasterization & Pixel Retention
// =========================================================================

#[test]
fn test_persistent_canvas_retains_untouched_pixels() {
    let mut engine = Engine::headless(400, 400);
    let color_sig = engine.signal(Color::rgba(0.0, 0.0, 1.0, 1.0)); // Blue

    // Two distinct boxes:
    // Box A (left, stationary, RED): (0, 0, 100, 100)
    // Box B (right, dynamic, BLUE):  (200, 0, 100, 100)
    engine.mount(group().direction(Direction::Horizontal).width(400.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        group().margin(Margin::left(100.0)).width(100.0).height(100.0).fill(color_sig.clone()),
    ]));
    engine.frame();

    // Verify Box A rendered red and Box B rendered blue on the persistent canvas
    {
        let canvas = engine.canvas();
        let pixel_a = canvas.pixel(50, 50).expect("Pixel A must exist");
        assert_eq!(pixel_a.red(), 255);
        assert_eq!(pixel_a.green(), 0);
        assert_eq!(pixel_a.blue(), 0);

        let pixel_b = canvas.pixel(250, 50).expect("Pixel B must exist");
        assert_eq!(pixel_b.red(), 0);
        assert_eq!(pixel_b.blue(), 255);
    }

    // Frame 2: Mutate Box B's color to Green
    color_sig.set(Color::rgba(0.0, 1.0, 0.0, 1.0));
    engine.frame();

    // Verify:
    // 1. Box A was NOT in the damage region
    let damage = engine.current_damage();
    let box_a_rect = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
    assert!(
        !damage.rects().iter().any(|r| r.intersects(&box_a_rect)),
        "Box A must not be damaged when Box B changes"
    );

    // 2. Box A's pixels were retained on the persistent canvas without re-drawing
    let canvas = engine.canvas();
    let pixel_a = canvas.pixel(50, 50).unwrap();
    assert_eq!(pixel_a.red(), 255, "Box A's red pixel must be preserved untouched");
    assert_eq!(pixel_a.green(), 0);
    assert_eq!(pixel_a.blue(), 0);

    // 3. Box B's pixels were correctly updated to Green
    let pixel_b = canvas.pixel(250, 50).unwrap();
    assert_eq!(pixel_b.red(), 0);
    assert_eq!(pixel_b.green(), 255, "Box B must now be green");
    assert_eq!(pixel_b.blue(), 0);
}

// =========================================================================
// 2.5 Window Resize & Full-Damage Reset
// =========================================================================
#[test]
fn test_window_resize_forces_full_window_damage() {
    let mut engine = Engine::headless(400, 300);
    engine.mount(group().fill(Color::BLACK));
    engine.frame();

    // Resize to 800x600
    engine.resize(800, 600);
    engine.frame();

    let damage = engine.current_damage();
    assert_eq!(damage.rects().len(), 1);
    assert_eq!(
        damage.rects()[0],
        ResolvedRect::new(0.0, 0.0, 800.0, 600.0),
        "Resize must immediately force full window damage"
    );
    assert_eq!(engine.canvas().width(), 800);
    assert_eq!(engine.canvas().height(), 600);
}


// =========================================================================
// Stress Tests: Heavy Drop Shadows & Modal Overlays
// =========================================================================

// In crates/aurora/tests/stage2_tests.rs:
// Replace the stress tests section at the bottom of the file:

// =========================================================================
// Stress Tests: Heavy Drop Shadows & Modal Overlays
// =========================================================================

#[test]
fn test_shadow_blur_fringe_damage_expansion_and_cleanup() {
    let mut engine = Engine::headless(600, 600);
    let spacer_width = engine.signal(100.0f32);

    // Row containing a spacer and a box with heavy drop shadow (blur=10.0, offset_y=10.0 -> kernel pad = 30px)
    engine.mount(group().direction(Direction::Horizontal).width(600.0).height(600.0).fill(Color::WHITE).children([
        group().width(spacer_width.clone()).height(10.0),
        group()
            .margin(Margin::top(100.0))
            .width(100.0).height(100.0)
            .fill(Color::BLACK)
            .shadows([Shadow::outer(0.0, 10.0, 10.0, Color::rgba(0.0, 0.0, 0.0, 0.5))]),
    ]));
    engine.frame();

    // Verify shadow blur fell onto white background outside the 100x100 box
    // Pixel directly below the box (at y=205, inside the blur halo)
    {
        let canvas = engine.canvas();
        let blur_pixel = canvas.pixel(150, 205).expect("Shadow fringe pixel must exist");
        assert!(blur_pixel.red() < 255, "Shadow blur must darken the background");
    }

    // Move the box from x=100 to x=300 by widening the preceding spacer
    spacer_width.set(300.0);
    engine.frame();

    let damage = engine.current_damage();

    // 1. Damage must encompass the 30px blur fringe of the old position (x=70..230, y=70..240)
    let old_shadow_perimeter = ResolvedRect::new(70.0, 70.0, 160.0, 170.0);
    assert!(
        damage.rects().iter().any(|r| r.intersects(&old_shadow_perimeter)),
        "Damage must encompass the 3σ blur fringe of the moved shadow"
    );

    // 2. The old shadow location (y=205) must now be cleanly restored to pure White
    let canvas = engine.canvas();
    let restored_pixel = canvas.pixel(150, 205).expect("Restored pixel must exist");
    assert_eq!(
        restored_pixel.red(), 255,
        "Old shadow blur halo must be completely cleared and restored to white"
    );
    assert_eq!(restored_pixel.green(), 255);
    assert_eq!(restored_pixel.blue(), 255);
}

#[test]
fn test_screen_overlay_modal_unmount_restores_underlying_content() {
    let mut engine = Engine::headless(800, 600);

    let build_tree = |show_modal: bool| {
        let mut children = vec![
            group()
                .margin(Margin::left(300.0))
                .margin(Margin::top(200.0))
                .width(200.0).height(200.0)
                .fill(Color::GREEN)
                .into_element(),
        ];

        if show_modal {
            children.push(
                group()
                    .margin(Margin::left(350.0))
                    .margin(Margin::top(250.0))
                    .width(100.0).height(100.0)
                    .fill(Color::BLUE)
                    .z_index(100)
                    .overlay(true).z_index(100)
                    .into_element(),
            );
        }

        group().width(800.0).height(600.0).fill(Color::WHITE).children(children)
    };

    // Frame 1: Mount with Modal visible
    engine.mount(build_tree(true));
    engine.frame();

    // Center pixel (400, 300) must be BLUE (modal on top of green card)
    {
        let canvas = engine.canvas();
        let center_pixel = canvas.pixel(400, 300).expect("Center pixel must exist");
        assert_eq!(center_pixel.blue(), 255, "Modal must render on top in blue");
        assert_eq!(center_pixel.green(), 0);
    }

    // Frame 2: Close/unmount the modal
    engine.mount(build_tree(false));
    engine.frame();

    let damage = engine.current_damage();
    let modal_bounds = ResolvedRect::new(350.0, 250.0, 100.0, 100.0);

    // 1. Damage must cover the unmounted modal's area
    assert!(
        damage.rects().iter().any(|r| r.intersects(&modal_bounds)),
        "Unmounting modal must damage its former screen bounds"
    );

    // 2. Underlying green card must be cleanly restored where the modal was
    let canvas = engine.canvas();
    let restored_pixel = canvas.pixel(400, 300).expect("Restored pixel must exist");
    assert_eq!(
        restored_pixel.green(), 255,
        "Underlying green card must be restored when modal closes"
    );
    assert_eq!(restored_pixel.blue(), 0, "No blue modal pixels must remain");
}

#[test]
fn test_window_resize_preserves_canvas_and_damages_exposed_strips() {
    let mut engine = Engine::headless(400, 300);
    // Draw a stationary red box in the top-left corner
    engine.mount(group().width(400.0).height(300.0).children([
        group().width(100.0).height(100.0).fill(Color::rgba(1.0, 0.0, 0.0, 1.0)),
    ]));
    engine.frame();

    // Verify top-left box is red
    {
        let canvas = engine.canvas();
        let pixel = canvas.pixel(50, 50).unwrap();
        assert_eq!(pixel.red(), 255);
    }

    // Resize to 800x600 (expands right and bottom)
    engine.resize(800, 600);
    engine.frame();

    let damage = engine.current_damage();
    assert!(!damage.is_empty(), "Resize must generate damage for exposed strips");

    // 1. The stationary 100x100 box at (0, 0) MUST NOT be in the damage region
    let stationary_box = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
    assert!(
        !damage.rects().iter().any(|r| r.intersects(&stationary_box)),
        "Stationary elements must NOT be damaged on resize"
    );

    // 2. The stationary box's red pixels must be preserved in the new canvas
    let canvas = engine.canvas();
    assert_eq!(canvas.width(), 800);
    assert_eq!(canvas.height(), 600);

    let preserved_pixel = canvas.pixel(50, 50).unwrap();
    assert_eq!(
        preserved_pixel.red(), 255,
        "Stationary pixels must be preserved across resize via scanline copy"
    );
}
