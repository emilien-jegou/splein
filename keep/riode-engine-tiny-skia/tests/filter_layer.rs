// Effect-layer raster: a `BeginLayer { effects }` renders content into an
// offscreen, applies the stack, then composites. These tests check the
// plumbing end-to-end (no panic, blur spreads alpha past the rect, a color
// matrix tints the result) — not visual polish.

use riode::foundation::{
    Appearance, Blur, Color, DamageRegion, EffectStack, Fill, Filter,
    IntoEffect, Point, Radius, ResolvedRect,
};
use riode::scene::{LayerId, Scene, SceneChunk, SceneCommand};
use riode_tiny_skia::renderer::TinySkiaRenderer;

fn full_damage(w: u32, h: u32) -> DamageRegion {
    let mut damage = DamageRegion::new();
    damage.push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
    damage
}

/// Layer with a blur filter containing one solid rect.
fn blurred_rect_scene(rect: ResolvedRect, sigma: f32) -> Scene {
    let mut scene = Scene::new();
    let mut chunk = SceneChunk::new(rect, Point::ZERO);
    chunk.push(SceneCommand::BeginLayer {
        id: LayerId(1),
        rect,
        radius: Radius::ZERO,
        layer_effects: Some(EffectStack::from(vec![Blur::new(sigma).into_effect()])),
        material: None,
        shadow_pad: 0.0,
    });
    chunk.push(SceneCommand::DrawRect {
        rect,
        appearance: Appearance {
            fill: Some(Fill::Solid(Color::RED)),
            ..Default::default()
        },
    });
    chunk.push(SceneCommand::EndLayer { id: LayerId(1) });
    scene.push_chunk(chunk);
    scene
}

#[test]
fn blurred_layer_spreads_alpha_past_the_rect() {
    let (w, h) = (96, 96);
    // Rect inset by 24 px so the blur tail has room inside the canvas.
    let rect = ResolvedRect::new(24.0, 24.0, 48.0, 48.0);
    let scene = blurred_rect_scene(rect, 4.0);

    let mut r = TinySkiaRenderer::new();
    r.resize(w, h);
    r.render_damage(&scene, &full_damage(w, h)).unwrap();
    let canvas = r.canvas();

    // Center: fully opaque red (the fill, unblurred well inside).
    let center = canvas.pixel(48, 48).unwrap();
    assert_eq!(center.alpha(), 255, "center of the fill must stay opaque");
    assert_eq!((center.red(), center.green(), center.blue()), (255, 0, 0));

    // Just outside the rect edge (x=23, one px left of the fill): the blur
    // must have spread alpha there. Without the filter layer this would be
    // fully transparent.
    let edge = canvas.pixel(23, 48).unwrap();
    assert!(
        edge.alpha() > 0,
        "blur must spread alpha past the rect edge (got alpha {})",
        edge.alpha()
    );

    // Far corner: the 4σ tail should have faded to ~0.
    let corner = canvas.pixel(2, 2).unwrap();
    assert_eq!(corner.alpha(), 0, "far corner must stay clear");
}

#[test]
fn color_filter_tints_the_layer() {
    let (w, h) = (64, 64);
    let rect = ResolvedRect::new(16.0, 16.0, 32.0, 32.0);
    let mut scene = Scene::new();
    let mut chunk = SceneChunk::new(rect, Point::ZERO);
    chunk.push(SceneCommand::BeginLayer {
        id: LayerId(2),
        rect,
        radius: Radius::ZERO,
        layer_effects: Some(Filter::saturate(0.0).into_effects()),
        material: None,
        shadow_pad: 0.0,
    });
    chunk.push(SceneCommand::DrawRect {
        rect,
        appearance: Appearance {
            fill: Some(Fill::Solid(Color::RED)),
            ..Default::default()
        },
    });
    chunk.push(SceneCommand::EndLayer { id: LayerId(2) });
    scene.push_chunk(chunk);

    let mut r = TinySkiaRenderer::new();
    r.resize(w, h);
    r.render_damage(&scene, &full_damage(w, h)).unwrap();
    let center = r.canvas().pixel(32, 32).unwrap();

    assert_eq!(center.alpha(), 255, "saturate(0) preserves alpha");
    // saturate(0) maps to luminance grey: R row is [0.213, 0.715, 0.072].
    let lum = (0.213f32 * 255.0).round() as u8;
    assert!(
        center.red().abs_diff(lum) <= 2,
        "saturate(0) should grey out red (got red {})",
        center.red()
    );
}

#[test]
fn plain_layer_still_clips() {
    // A layer with no filter must behave like the pre-filter clip layer:
    // content is clipped to the layer rect.
    let (w, h) = (64, 64);
    let layer_rect = ResolvedRect::new(16.0, 16.0, 32.0, 32.0);
    // The draw rect is bigger than the layer: the excess must be clipped.
    let big = ResolvedRect::new(0.0, 0.0, 64.0, 64.0);
    let mut scene = Scene::new();
    let mut chunk = SceneChunk::new(big, Point::ZERO);
    chunk.push(SceneCommand::BeginLayer {
        id: LayerId(3),
        rect: layer_rect,
        radius: Radius::ZERO,
        layer_effects: None,
        material: None,
        shadow_pad: 0.0,
    });
    chunk.push(SceneCommand::DrawRect {
        rect: big,
        appearance: Appearance {
            fill: Some(Fill::Solid(Color::RED)),
            ..Default::default()
        },
    });
    chunk.push(SceneCommand::EndLayer { id: LayerId(3) });
    scene.push_chunk(chunk);

    let mut r = TinySkiaRenderer::new();
    r.resize(w, h);
    r.render_damage(&scene, &full_damage(w, h)).unwrap();
    let canvas = r.canvas();

    assert_eq!(canvas.pixel(32, 32).unwrap().alpha(), 255, "inside the clip");
    assert_eq!(canvas.pixel(4, 4).unwrap().alpha(), 0, "outside the clip");
}
