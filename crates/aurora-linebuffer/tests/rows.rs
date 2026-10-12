// Row-equivalence: linebuffer rows must byte-match the tile backend canvas
// for the same scene and damage (same executor, different tiling).

use aurora::foundation::{Appearance, Color, DamageRegion, Fill, Point, ResolvedRect};
use aurora::scene::{Scene, SceneChunk, SceneCommand};
use aurora_linebuffer::{FramebufferSink, LineBufferBackend, LineSink};

fn solid_rect_scene() -> Scene {
    let mut scene = Scene::new();
    let mut chunk = SceneChunk::new(
        ResolvedRect::new(4.0, 2.0, 20.0, 10.0),
        Point::ZERO,
    );
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(4.0, 2.0, 20.0, 10.0),
        appearance: Appearance {
            fill: Some(Fill::Solid(Color::RED)),
            ..Default::default()
        },
    });
    // Second chunk with a chunk-local offset, exercising placement.
    let mut moved = SceneChunk::new(
        ResolvedRect::new(30.0, 6.0, 12.0, 8.0),
        Point::new(30.0, 6.0),
    );
    moved.push(SceneCommand::PushOffset(Point::ZERO));
    moved.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(0.0, 0.0, 12.0, 8.0),
        appearance: Appearance {
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        },
    });
    moved.push(SceneCommand::PopOffset);
    scene.push_chunk(chunk);
    scene.push_chunk(moved);
    scene
}

fn full_damage(w: u32, h: u32) -> DamageRegion {
    let mut damage = DamageRegion::new();
    damage.push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
    damage
}

#[test]
fn rows_match_tile_backend() {
    let (w, h) = (64, 32);
    let scene = solid_rect_scene();
    let damage = full_damage(w, h);

    let mut reference = aurora_tiny_skia::renderer::TinySkiaRenderer::new();
    reference.resize(w, h);
    reference.render_damage(&scene, &damage).unwrap();
    let expected = reference.canvas().data().to_vec();

    let mut backend = LineBufferBackend::new(w, h);
    let mut sink = FramebufferSink::new(w, h);
    backend
        .render_damage(&scene, &damage, &mut sink)
        .unwrap();
    assert_eq!(sink.flushed(), 1);
    assert_eq!(
        sink.buffer(),
        expected.as_slice(),
        "linebuffer rows must byte-match the tile backend"
    );
}

#[test]
fn empty_damage_pushes_nothing() {
    let (w, h) = (64, 32);
    let scene = solid_rect_scene();
    let damage = DamageRegion::new();
    let mut backend = LineBufferBackend::new(w, h);
    let mut sink = FramebufferSink::new(w, h);
    backend.render_damage(&scene, &damage, &mut sink).unwrap();
    assert_eq!(sink.flushed(), 0);
    assert!(sink.buffer().iter().all(|&b| b == 0));
}
