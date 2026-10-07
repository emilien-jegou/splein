// Single responsibility: Chunk-local emission keeps paint content reusable across moves.
//
// When only placement changes (scroll, offset animation), a chunk's command
// stream must be byte-identical to the previous frame — same content hash —
// with only `abs_origin` differing. The consumer then re-appends the cached
// encoding under the new placement instead of recompiling.

use aurora::dsl::*;
use aurora::foundation::{Color, Point, Size};
use aurora::prelude::Direction;
use aurora::runtime::Engine;
use aurora::scene::command::hash_commands;
use std::collections::HashMap;

fn chunk_map(engine: &Engine) -> HashMap<u64, (f32, f32)> {
    let mut map = HashMap::new();
    for chunk in engine.scene().chunks.iter() {
        if chunk.commands.is_empty() {
            continue;
        }
        map.insert(
            hash_commands(&chunk.commands),
            (chunk.abs_origin.x, chunk.abs_origin.y),
        );
    }
    map
}

#[test]
#[ntest::timeout(5_000)]
fn offset_only_motion_keeps_content_hash_stable() {
    let mut engine = Engine::headless(400, 200);
    let spacer = engine.signal(0.0_f32);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group().width(spacer.clone()).height(200.0),
                // Stacking context (z_index) so the box owns its chunk:
                // only its placement moves, never its content.
                group()
                    .width(Size::Fixed(100.0))
                    .height(100.0)
                    .fill(Color::BLUE)
                    .z_index(1),
            )),
    );
    engine.frame();
    let before = chunk_map(&engine);

    // Shift the blue box right by 50px via the spacer: its content is
    // untouched, only placement moves.
    spacer.set(50.0);
    engine.frame();
    let after = chunk_map(&engine);

    let moved = before
        .iter()
        .filter_map(|(hash, (x0, y0))| {
            after
                .get(hash)
                .filter(|(x1, y1)| (x1 - x0).abs() > 0.5 || (y1 - y0).abs() > 0.5)
                .map(|(x1, _)| (*x0, *x1))
        })
        .collect::<Vec<_>>();
    assert!(
        !moved.is_empty(),
        "at least one chunk must survive an offset-only motion with identical content"
    );
    // The survivor is the blue box: content identical, origin shifted +50x.
    assert!(
        moved.iter().any(|(x0, x1)| (x1 - x0 - 50.0).abs() < 1.0),
        "moved chunk origins must shift exactly with the spacer, got {moved:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn root_offset_is_rebased_to_local_zero() {
    let mut engine = Engine::headless(400, 200);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group().width(Size::Fixed(200.0)).height(200.0),
                group()
                    .width(Size::Fixed(100.0))
                    .height(100.0)
                    .fill(Color::RED),
            )),
    );
    engine.frame();

    // Every chunk stream opens at its own origin: the first offset is local.
    for chunk in engine.scene().chunks.iter() {
        let first_offset = chunk
            .commands
            .iter()
            .find_map(|cmd| match cmd {
                aurora::scene::SceneCommand::PushOffset(p) => Some(Point::new(p.x, p.y)),
                _ => None,
            });
        if let Some(p) = first_offset {
            assert!(
                p.x.abs() < 0.01 && p.y.abs() < 0.01,
                "root frame offset must be chunk-local zero, got ({}, {})",
                p.x,
                p.y
            );
            break;
        }
    }
}
