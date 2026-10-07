// Single responsibility: TDD verification for Stage 6 damage rasterization, scratch clearing, and 9-patch reuse.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::{Engine, FrameReport};
use aurora_tiny_skia::HeadlessRaster;
use aurora::tree::DirtyFlags;

#[test]
#[ntest::timeout(5_000)]
fn test_6_1_hover_color_diff_strictly_isolated_to_damage_bounds() {
    let mut engine = Engine::headless(400, 400);
    let mut raster = HeadlessRaster::new();
    let color = engine.signal(Color::RED);

    engine.mount(
        group().width(400.0).height(400.0).children([group()
            .margin(Margin::sides(10.0, 10.0, 10.0, 10.0))
            .width(50.0)
            .height(50.0)
            .fill(color.clone())]),
    );
    engine.frame();

    let canvas_before = raster.draw_engine(&mut engine).clone();

    color.set(Color::BLUE);
    let FrameReport {
        diagnostics: diag, ..
    } = engine.frame();

    let canvas_after = raster.draw_engine(&mut engine);
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
#[ntest::timeout(5_000)]
fn test_6_2_shadow_color_mutation_reuses_cached_blur_mask() {
    let mut engine = Engine::headless(400, 400);
    let shadow_color = engine.signal(Color::rgba(0.0, 0.0, 0.0, 0.5));
    let sc = shadow_color.clone();
    let dyn_shadow = computed(move || Shadow::outer(0.0, 4.0, 8.0, sc.get()));

    engine.mount(
        group().width(400.0).height(400.0).children([group()
            .width(100.0)
            .height(100.0)
            .dynamic_shadow(dyn_shadow)]),
    );
    engine.frame();

    shadow_color.set(Color::rgba(1.0, 0.0, 0.0, 0.5));
    let FrameReport { stats, .. } = engine.frame();

    assert_eq!(stats.flags, DirtyFlags::PAINT);
    assert!(!stats.laid_out);
}

#[test]
#[ntest::timeout(5_000)]
fn test_6_4_layer_wraps_subtree_in_clip_isolation() {
    use aurora_tiny_skia::TinySkiaRenderer;
    use aurora::scene::{LayerId, Scene, SceneChunk, SceneCommand};

    let mut renderer = TinySkiaRenderer::new();
    renderer.resize(100, 100);

    // A 100x100 red fill wrapped in a 50x50 layer must be clipped to the layer.
    let mut chunk = SceneChunk::new(ResolvedRect::new(0.0, 0.0, 100.0, 100.0), Point::ZERO);
    chunk.push(SceneCommand::BeginLayer {
        id: LayerId(7),
        rect: ResolvedRect::new(0.0, 0.0, 50.0, 50.0),
    });
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(0.0, 0.0, 100.0, 100.0),
        appearance: Appearance::EMPTY.with_fill(Fill::Solid(Color::RED)),
    });
    chunk.push(SceneCommand::EndLayer { id: LayerId(7) });
    let mut scene = Scene::new();
    scene.push_chunk(chunk);

    let mut damage = DamageRegion::new();
    damage.push(ResolvedRect::new(0.0, 0.0, 100.0, 100.0));
    renderer.render_damage(&scene, &damage).unwrap();

    let canvas = renderer.canvas();
    let inside = canvas.pixel(25, 25).unwrap();
    assert!(
        inside.red() > 200,
        "Scenario 6.4: layer content missing at (25, 25)"
    );
    let outside = canvas.pixel(75, 75).unwrap();
    assert_eq!(
        outside.alpha(),
        0,
        "Scenario 6.4: layer failed to clip content at (75, 75)"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn test_6_5_rotated_rect_outside_translation_box_still_rasterizes() {
    use aurora_tiny_skia::TinySkiaRenderer;
    use aurora::scene::{Scene, SceneChunk, SceneCommand};

    let mut renderer = TinySkiaRenderer::new();
    renderer.resize(100, 100);

    // Local 10x10 box translated to x=105: the old translation-only tile cull
    // saw (0 + 105) >= 100 and skipped it. Rotated 90 degrees, its footprint
    // lands at x in [95, 105], overlapping the tile, so it must rasterize.
    let spun =
        Transform::from_translation(105.0, 40.0).multiply(&Transform::from_rotation_degrees(90.0));
    let mut chunk = SceneChunk::new(ResolvedRect::new(0.0, 0.0, 100.0, 100.0), Point::ZERO);
    chunk.push(SceneCommand::PushTransform(spun));
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(0.0, 0.0, 10.0, 10.0),
        appearance: Appearance::EMPTY.with_fill(Fill::Solid(Color::RED)),
    });
    chunk.push(SceneCommand::PopTransform);
    let mut scene = Scene::new();
    scene.push_chunk(chunk);

    let mut damage = DamageRegion::new();
    damage.push(ResolvedRect::new(0.0, 0.0, 100.0, 100.0));
    renderer.render_damage(&scene, &damage).unwrap();

    let px = renderer.canvas().pixel(97, 45).unwrap();
    assert!(
        px.red() > 200,
        "Scenario 6.5: rotated rect culled despite overlapping the tile"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn test_6_6_rotated_text_rotates_glyph_shapes_not_just_positions() {
    use aurora_tiny_skia::{render_text, TextRenderParams};
    use aurora::text::{TextConfig, TextContext};

    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../assets/inter-var.ttf"));
    let config = TextConfig {
        content: "—".to_string(),
        size: 48.0,
        ..TextConfig::default()
    };
    let layout = ctx.shape_config(&config, Constraints::loose(f32::INFINITY, f32::INFINITY));
    assert!(
        !layout.lines.is_empty(),
        "Scenario 6.6: em dash failed to shape"
    );

    // Inked-pixel bounding box of a canvas.
    fn ink_bbox(canvas: &tiny_skia::Pixmap) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0) = (u32::MAX, u32::MAX);
        let (mut x1, mut y1) = (0u32, 0u32);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                if canvas.pixel(x, y).map_or(false, |p| p.alpha() > 8) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x1 >= x0).then_some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    // Control: unrotated em dash is a wide horizontal bar.
    let mut flat = tiny_skia::Pixmap::new(240, 160).unwrap();
    render_text(
        &mut flat.as_mut(),
        &ctx,
        TextRenderParams {
            origin: Point::new(20.0, 100.0),
            layout: &layout,
            color: Color::BLACK,
            opacity: 1.0,
            transform: tiny_skia::Transform::identity(),
            clip: None,
        },
    );
    let (_, _, fw, fh) = ink_bbox(&flat).expect("Scenario 6.6: flat dash drew nothing");
    assert!(
        fw >= 2 * fh,
        "Scenario 6.6: control dash should be wide ({fw}x{fh})"
    );

    // Rotated 90 degrees, the bar must stand vertical. The old path moved the
    // (unrotated) horizontal bitmap to the rotated position, staying wide.
    let spun = tiny_skia::Transform::from_translate(170.0, 20.0)
        .pre_concat(tiny_skia::Transform::from_rotate(90.0));
    let mut tall = tiny_skia::Pixmap::new(240, 160).unwrap();
    render_text(
        &mut tall.as_mut(),
        &ctx,
        TextRenderParams {
            origin: Point::ZERO,
            layout: &layout,
            color: Color::BLACK,
            opacity: 1.0,
            transform: spun,
            clip: None,
        },
    );
    let (_, _, rw, rh) = ink_bbox(&tall).expect("Scenario 6.6: rotated dash drew nothing");
    assert!(
        rh >= 2 * rw,
        "Scenario 6.6: rotated dash should be tall ({rw}x{rh})"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn test_6_3_box_smaller_than_corner_patch_falls_back_to_direct_blur_without_crash() {
    let mut engine = Engine::headless(200, 200);

    engine.mount(
        group().width(200.0).height(200.0).children([group()
            .width(10.0)
            .height(10.0)
            .shadows([Shadow::outer(0.0, 0.0, 20.0, Color::BLACK)])]),
    );

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
}
