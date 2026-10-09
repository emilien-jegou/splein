// Single responsibility: TDD verification for Stage 6 damage rasterization, scratch clearing, and 9-patch reuse.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::{Engine, FrameReport};
use aurora_tiny_skia::HeadlessRaster;
use aurora::tree::DirtyFlags;

#[test]
#[ntest::timeout(5_000)]
fn test_6_2_shadow_color_mutation_reuses_cached_blur_mask() {
    let mut engine = Engine::headless(400, 400);
    let shadow_color = engine.signal(Color::rgba(0.0, 0.0, 0.0, 0.5));
    let sc = shadow_color.clone();
    let dyn_shadow = computed(move || Shadow::web_like(0.0, 4.0, 8.0, sc.get()));

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
fn test_6_3_box_smaller_than_corner_patch_falls_back_to_direct_blur_without_crash() {
    let mut engine = Engine::headless(200, 200);

    engine.mount(
        group().width(200.0).height(200.0).children([group()
            .width(10.0)
            .height(10.0)
            .shadow(Shadow::web_like(0.0, 0.0, 20.0, Color::BLACK))]),
    );

    let FrameReport { stats, .. } = engine.frame();
    assert!(stats.laid_out);
}
