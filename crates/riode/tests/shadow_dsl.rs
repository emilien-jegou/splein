// Single responsibility: Coverage for the shadow DSL cheat sheet — every
// documented construction form builds the specified values, reactive
// shadows stay paint-only, and light overrides cascade through subtrees.

use riode::dsl::*;
use riode::foundation::*;
use riode::reactive::*;
use riode::runtime::Engine;
use riode::tree::DirtyFlags;
use riode_tiny_skia::HeadlessRaster;

fn built(def: GroupDef) -> (Option<Shadow>, Option<LightEnvironment>) {
    let shadow = def.shadow.map(|p| p.evaluate());
    (shadow, def.light_override)
}

#[test]
fn cheat_sheet_1_raw_elevation() {
    let (shadow, light) = built(group().shadow(8.0));
    let s = shadow.expect("shadow present");
    assert_eq!(s.elevation, 8.0);
    assert_eq!(s.blend, BlendMode::Multiply);
    assert_eq!(s.layers().len(), 1);
    assert_eq!(light, None);
}

#[test]
fn cheat_sheet_2_elevation_with_tint() {
    let color = Color::hex(0x7A3D8F).with_alpha(0.2);
    let (shadow, _) = built(group().shadow((8.0, color)));
    let s = shadow.expect("shadow present");
    assert_eq!(s.elevation, 8.0);
    assert_eq!(s.color, color);
}

#[test]
fn cheat_sheet_3_full_builder() {
    let (shadow, _) = built(
        group().shadow(
            Shadow::elevation(16.0)
                .spread(-2.0)
                .falloff(FalloffCurve::Physical)
                .alpha()
                .multiply(),
        ),
    );
    let s = shadow.expect("shadow present");
    assert_eq!(s.spread, -2.0);
    assert_eq!(s.shape, ShadowShape::Alpha);
    assert_eq!(s.blend, BlendMode::Multiply);
    assert!(s
        .layers()
        .iter()
        .all(|l| l.falloff == FalloffCurve::Physical));
}

#[test]
fn cheat_sheet_4_reactive_shadow_is_paint_only() {
    let mut engine = Engine::headless(400, 400);
    let elevation = engine.signal(8.0f32);
    let blend_on = engine.signal(false);
    let (e, b) = (elevation.clone(), blend_on.clone());
    engine.mount(group().width(100.0).height(100.0).shadow(computed(move || {
        Shadow::elevation(e.get()).multiply_if(b.get())
    })));
    engine.frame();

    elevation.set(16.0);
    blend_on.set(true);
    let report = engine.frame();
    assert_eq!(report.stats.flags, DirtyFlags::PAINT);
    assert!(!report.stats.laid_out, "shadow edits must not lay out");
}

#[test]
fn cheat_sheet_5a_web_like_polyfill() {
    let (shadow, _) = built(group().shadow(Shadow::web_like(
        0.0,
        4.0,
        8.0,
        0.0,
        Color::rgba8(0, 0, 0, 80),
    )));
    let s = shadow.expect("shadow present");
    assert_eq!(s.blend, BlendMode::Normal);
    assert_eq!(s.spread, 0.0);
    assert_eq!(s.layers().len(), 1);
    let (ox, oy) = s.resolve_offset(&LightEnvironment::DEFAULT);
    assert_eq!((ox, oy), (0.0, 4.0));
}

#[test]
fn cheat_sheet_5b_stacked_drops() {
    let (shadow, _) = built(group().shadow(
        Shadow::layered(Color::hex(0x7A3D8F).with_alpha(0.15)).drops(&[
            (0.5, 1.0, 1.0),
            (1.0, 2.0, 2.0),
            (2.0, 4.0, 4.0),
            (4.0, 8.0, 8.0),
            (8.0, 16.0, 16.0),
        ]),
    ));
    let s = shadow.expect("shadow present");
    assert_eq!(s.layers().len(), 5);
    assert_eq!(s.layers()[4].offset, Some((8.0, 16.0)));
}

/// Dark-pixel bounding boxes of the same card under two ambient lights:
/// the override must steer the shadow, proving end-to-end cascade
/// (DSL override -> damage stash -> emitted DrawShadow -> pixels).
fn shadow_bbox(engine: &mut Engine, raster: &mut HeadlessRaster) -> (i32, i32, u32, u32) {
    let canvas = raster.draw_engine(engine);
    let (w, h) = (canvas.width() as i32, canvas.height() as i32);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let px = &canvas.data()[((y * w + x) * 4) as usize..][..4];
            // Darkened (shadowed) pixel on the light backdrop.
            if px[0] < 200 && px[3] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

fn card_under(light: Option<LightEnvironment>) -> impl IntoElement {
    let card = group()
        .width(100.0)
        .height(100.0)
        .fill(Color::hex(0xE8E6E1))
        .shadow(12.0);
    let inner = group()
        .width(400.0)
        .height(400.0)
        .fill(Color::hex(0xE8E6E1))
        .children([card]);
    match light {
        Some(env) => group()
            .width(400.0)
            .height(400.0)
            .light(env)
            .children([inner])
            .into_element(),
        None => inner.into_element(),
    }
}

#[test]
#[ntest::timeout(5_000)]
fn light_override_steers_shadow() {
    let mut raster = HeadlessRaster::new();
    let mut base = Engine::headless(400, 400);
    base.mount(card_under(None));
    base.frame();
    let (_, y_default, _, _) = shadow_bbox(&mut base, &mut raster);

    let mut over = Engine::headless(400, 400);
    over.mount(card_under(Some(LightEnvironment::from_angles(270.0, 45.0))));
    over.frame();
    let (_, y_over, _, _) = shadow_bbox(&mut over, &mut raster);

    // Straight-up light throws the shadow upward vs the default down-left.
    assert!(
        y_over < y_default,
        "override light must move the shadow up ({y_over} vs {y_default})"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn child_override_wins_over_parent() {
    let mut raster = HeadlessRaster::new();
    let plain = || {
        group()
            .width(100.0)
            .height(100.0)
            .fill(Color::hex(0xE8E6E1))
            .shadow(12.0)
    };
    let mut a = Engine::headless(400, 400);
    a.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::hex(0xE8E6E1))
            .light(LightEnvironment::from_angles(90.0, 45.0))
            .children([plain()]),
    );
    a.frame();
    let bbox_a = shadow_bbox(&mut a, &mut raster);

    let mut b = Engine::headless(400, 400);
    b.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::hex(0xE8E6E1))
            .light(LightEnvironment::from_angles(90.0, 45.0))
            .children([plain().light(LightEnvironment::from_angles(270.0, 45.0))]),
    );
    b.frame();
    let bbox_b = shadow_bbox(&mut b, &mut raster);

    assert_ne!(bbox_a, bbox_b, "child override must win, got {bbox_a:?}");
    let mut c = Engine::headless(400, 400);
    c.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::hex(0xE8E6E1))
            .children([plain().light(LightEnvironment::from_angles(270.0, 45.0))]),
    );
    c.frame();
    assert_eq!(
        shadow_bbox(&mut c, &mut raster),
        bbox_b,
        "child override must match the no-parent case exactly"
    );
}
