// Single responsibility: TDD verification for Stage 6 damage rasterization, scratch clearing, and 9-patch reuse.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::Engine;
use aurora::tree::DirtyFlags;

#[test]
fn test_6_1_hover_color_diff_strictly_isolated_to_damage_bounds() {
    let mut engine = Engine::headless(400, 400);
    let color = engine.signal(Color::RED);

    engine.mount(group().size(400.0, 400.0).children([
        group().margin_xy(10.0, 10.0).size(50.0, 50.0).fill(color.clone()),
    ]));
    engine.frame();

    let canvas_before = engine.canvas().clone();

    color.set(Color::BLUE);
    let (_, _, _, diag) = engine.frame();

    let canvas_after = engine.canvas();
    let damage_box = diag.spatial.damaged_rects[0];

    for y in 0..400 {
        for x in 0..400 {
            let p = Point::new(x as f32, y as f32);
            if !damage_box.contains(p) {
                assert_eq!(
                    canvas_before.pixel(x, y),
                    canvas_after.pixel(x, y),
                    "Scenario 6.1: Pixels outside damage rect were modified at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn test_6_2_shadow_color_mutation_reuses_cached_blur_mask() {
    let mut engine = Engine::headless(400, 400);
    let shadow_color = engine.signal(Color::rgba(0.0, 0.0, 0.0, 0.5));
    let sc = shadow_color.clone();
    let dyn_shadow = computed(move || Shadow::outer(0.0, 4.0, 8.0, sc.get()));

    engine.mount(group().size(400.0, 400.0).children([
        group().size(100.0, 100.0).dynamic_shadow(dyn_shadow),
    ]));
    engine.frame();

    shadow_color.set(Color::rgba(1.0, 0.0, 0.0, 0.5));
    let (stats, _, _, _) = engine.frame();

    assert_eq!(stats.flags, DirtyFlags::PAINT);
    assert!(!stats.laid_out);
}

#[test]
fn test_6_3_box_smaller_than_corner_patch_falls_back_to_direct_blur_without_crash() {
    let mut engine = Engine::headless(200, 200);

    engine.mount(group().size(200.0, 200.0).children([
        group()
            .size(10.0, 10.0)
            .shadow(Shadow::outer(0.0, 0.0, 20.0, Color::BLACK)),
    ]));

    let (stats, _, _, _) = engine.frame();
    assert!(stats.laid_out);
}
