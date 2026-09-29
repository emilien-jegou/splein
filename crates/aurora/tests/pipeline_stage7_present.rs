// Single responsibility: TDD verification for Stage 7 presentation swapchain age and idle skipping.

use aurora::foundation::*;

#[test]
fn test_7_1_buffer_age_1_copies_only_active_frame_damage() {
    let mut ring = DamageRing::new();
    let full_window = ResolvedRect::new(0.0, 0.0, 800.0, 600.0);

    let d1 = ring.advance();
    d1.push(ResolvedRect::new(10.0, 10.0, 20.0, 20.0));

    let d2 = ring.advance();
    d2.push(ResolvedRect::new(50.0, 50.0, 30.0, 30.0));

    // Buffer Age 1: current frame damage only
    let age1 = ring.damage_for_age(1, full_window);
    assert_eq!(age1.rects().len(), 1);
    assert_eq!(age1.rects()[0], ResolvedRect::new(50.0, 50.0, 30.0, 30.0));
}

#[test]
fn test_7_2_buffer_age_2_unions_previous_two_frames() {
    let mut ring = DamageRing::new();
    let full_window = ResolvedRect::new(0.0, 0.0, 800.0, 600.0);

    let d1 = ring.advance();
    d1.push(ResolvedRect::new(10.0, 10.0, 20.0, 20.0));

    let d2 = ring.advance();
    d2.push(ResolvedRect::new(50.0, 50.0, 30.0, 30.0));

    // Buffer Age 2: must union Frame 2 and Frame 1
    let age2 = ring.damage_for_age(2, full_window);
    assert_eq!(age2.rects().len(), 2);
    assert!(age2.rects().iter().any(|r| r.x == 10.0));
    assert!(age2.rects().iter().any(|r| r.x == 50.0));
}

#[test]
fn test_7_3_buffer_age_0_forces_full_window_damage() {
    let mut ring = DamageRing::new();
    let full_window = ResolvedRect::new(0.0, 0.0, 800.0, 600.0);

    let d = ring.advance();
    d.push(ResolvedRect::new(10.0, 10.0, 20.0, 20.0));

    // Age 0 signifies uninitialized buffer -> full window repaint
    let age0 = ring.damage_for_age(0, full_window);
    assert_eq!(age0.rects().len(), 1);
    assert_eq!(age0.rects()[0], full_window);
}
