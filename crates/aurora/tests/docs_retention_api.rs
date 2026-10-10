// Single responsibility: Executable contract for docs/retention.md.
//
// The surface the retention guide documents exists and behaves as written.
// Assertions are deliberately shallow — pixel-exact behavior lives in
// damage_parity and chunk_locality; here the documented invariants must be
// literally true: idle frames emit nothing, moved chunks keep their bytes,
// and a change to one sibling never recompiles the other. The offscreen
// reveal test is ignored until its known gap is fixed (reason on the test).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aurora::dsl::*;
use aurora::foundation::{Color, Direction, Margin, Size};
use aurora::reactive::{Cx, ReactiveRuntime, Signal};
use aurora::runtime::{Engine, FrameReport};
use aurora::scene::command::hash_commands;
use aurora_tiny_skia::testing::Harness;

/// Content hash of every non-empty chunk, with the origin it was placed at.
fn chunk_map(engine: &Engine) -> HashMap<u64, (f32, f32, bool)> {
    let mut map = HashMap::new();
    for chunk in engine.scene().chunks.iter() {
        if chunk.commands.is_empty() {
            continue;
        }
        map.insert(
            hash_commands(&chunk.commands),
            (chunk.abs_origin.x, chunk.abs_origin.y, chunk.fresh),
        );
    }
    map
}

/// An idle second frame emits no commands and replays every chunk.
#[test]
#[ntest::timeout(5_000)]
fn idle_frame_reuses_everything() {
    let mut engine = Engine::headless(400, 200);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group()
                    .width(Size::Fixed(200.0))
                    .height(200.0)
                    .fill(Color::BLUE)
                    .z_index(1),
                group()
                    .width(Size::Fixed(100.0))
                    .height(100.0)
                    .fill(Color::RED)
                    .z_index(2),
            ))
            .into_element(),
    );
    engine.frame();
    let FrameReport {
        diagnostics: diag, ..
    } = engine.frame();

    assert_eq!(
        diag.compile.commands_emitted, 0,
        "an idle frame must not encode new commands"
    );
    assert_eq!(
        diag.compile.cached_chunks, diag.compile.chunks_emitted,
        "every emitted chunk must be a replay"
    );
    assert!(
        engine.scene().chunks.iter().all(|c| !c.fresh),
        "no chunk may report fresh work on an idle frame"
    );
}

/// Shifting a box by a whole number of pixels keeps its command bytes and
/// replays the chunk at the new origin with `fresh == false`.
#[test]
#[ntest::timeout(5_000)]
fn offset_motion_replays_with_exact_shift() {
    let mut engine = Engine::headless(400, 200);
    let spacer = engine.signal(0.0_f32);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group().width(spacer.clone()).height(200.0),
                group()
                    .width(Size::Fixed(100.0))
                    .height(100.0)
                    .fill(Color::BLUE)
                    .z_index(1),
            ))
            .into_element(),
    );
    engine.frame();
    let before = chunk_map(&engine);

    spacer.set(50.0);
    engine.frame();
    let after = chunk_map(&engine);

    let moved = before
        .iter()
        .filter_map(|(hash, (x0, y0, _))| {
            after
                .get(hash)
                .filter(|(x1, y1, _)| (x1 - x0).abs() > 0.5 || (y1 - y0).abs() > 0.5)
                .map(|(x1, _, fresh)| (*x0, *x1, *fresh))
        })
        .collect::<Vec<_>>();
    assert!(
        !moved.is_empty(),
        "a moved chunk must survive with identical command bytes"
    );
    for (x0, x1, fresh) in &moved {
        assert!(
            (x1 - x0 - 50.0).abs() < 1.0,
            "moved chunk origins must shift exactly with the spacer, got {moved:?}"
        );
        assert!(
            !fresh,
            "a chunk whose content did not change must replay, not recompile"
        );
    }
}

/// Changing one sibling's text recompiles exactly that sibling's chunk.
#[test]
#[ntest::timeout(5_000)]
fn sibling_content_change_recompiles_only_it() {
    let mut engine = Engine::headless(400, 200);
    let label = engine.signal("left".to_string());
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group()
                    .width(Size::Fixed(200.0))
                    .height(200.0)
                    .z_index(1)
                    .children([text(label.clone())
                        .size(14.0)
                        .color(Color::WHITE)
                        .into_element()])
                    .into_element(),
                group()
                    .width(Size::Fixed(200.0))
                    .height(200.0)
                    .fill(Color::RED)
                    .z_index(2)
                    .into_element(),
            ))
            .into_element(),
    );
    engine.frame();
    let before = chunk_map(&engine);

    label.set("left, edited".to_string());
    let FrameReport {
        diagnostics: diag, ..
    } = engine.frame();
    assert!(
        diag.compile.commands_emitted > 0,
        "the edited sibling must encode new commands"
    );
    let after = chunk_map(&engine);

    // The untouched right box (origin x ~= 200) keeps byte-identical content
    // and replays it.
    let right_before = before
        .iter()
        .filter(|(_, (x, _, _))| *x > 100.0)
        .map(|(h, _)| *h)
        .collect::<Vec<_>>();
    assert!(
        !right_before.is_empty(),
        "test setup must produce a right-side chunk"
    );
    for hash in right_before {
        let (_, _, fresh) = after
            .get(&hash)
            .expect("the untouched sibling's command bytes must survive its sibling's edit");
        assert!(!fresh, "the untouched sibling must replay, not recompile");
    }
}

/// A dirty offscreen row scrolled onscreen repaints exactly its revealed
/// pixels: one re-emit for the dirty row, replays everywhere else.
///
/// IGNORED — documents a real content-loss bug, not intended behavior:
/// `compile_node_hierarchy` culls offscreen non-context nodes mid-emit
/// (`hierarchy.rs`), but the enclosing stacking root still reaches the
/// flag-clear, so the ancestor replays fill-only content that never included
/// the dirty descendant, and nothing re-emits it on reveal. Un-ignore when
/// culled-dirty descendants force their context to stay dirty.
#[test]
#[ignore = "culled-dirty descendants are lost from replayed chunks; see docs/retention.md Known gap"]
#[ntest::timeout(5_000)]
fn dirty_offscreen_row_reveals_cleanly() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let dy = Signal::new(Rc::clone(&runtime), 0.0f32);
    let row4_text = Signal::new(Rc::clone(&runtime), "row four".to_string());

    let tree = {
        let dy = dy.clone();
        let row4_text = row4_text.clone();
        move || {
            // Rows are absolutely positioned: flex would shrink them into
            // the 200px viewport and nothing would ever sit offscreen, and
            // absolute placement derives from anchor+margins (not x/y), so
            // each row sits at i*60 via its top margin. The column carries
            // the scroll offset; rows sit at i*60.
            group()
                .width(400.0)
                .height(200.0)
                .y(dy.clone())
                .children(
                    (0..6)
                        .map(|i| {
                            let mut row = group()
                                .key(format!("row-{i}"))
                                .absolute()
                                .direction(Direction::Horizontal)
                                .width(Size::Fixed(400.0))
                                .height(Size::Fixed(60.0))
                                .margin(Margin::top(i as f32 * 60.0))
                                .fill(Color::hex(0x4F46E5))
                                .z_index(10 + i as i32);
                            if i == 4 {
                                row = row.children([text(row4_text.clone())
                                    .size(14.0)
                                    .color(Color::WHITE)
                                    .into_element()]);
                            }
                            row.into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_element()
        }
    };
    let mut h = Harness::from_runtime(
        Rc::clone(&runtime),
        Cx::new(Rc::clone(&runtime)).enter(tree),
        400,
        200,
    );
    h.step();
    h.step();

    // Dirty row 4 while it sits fully below the 200px fold.
    row4_text.set("row four, edited".to_string());
    h.step();
    h.damage_parity()
        .assert_no_strays(128, "editing an offscreen row");

    // Scroll the column up: row 4 (was y 240..300) lands at y 60..120.
    dy.set(-180.0);
    h.step();
    h.damage_parity()
        .assert_no_strays(128, "revealing a dirty offscreen row");

    // Row-height chunks only: the revealed row re-emitted, the rest replayed.
    let mut fresh_rows = 0usize;
    let mut replayed_rows = 0usize;
    for chunk in h.engine().scene().chunks.iter() {
        if chunk.commands.is_empty() || chunk.bounds.height > 100.0 {
            continue;
        }
        // Row chunks live at multiples of 60, shifted by the -180 scroll.
        let y = chunk.abs_origin.y;
        let is_row_band = y > -70.0 && ((y / 60.0).round() - y / 60.0).abs() < 0.05;
        if !is_row_band {
            continue;
        }
        if chunk.fresh {
            fresh_rows += 1;
            assert!(
                (y - 60.0).abs() < 2.0,
                "the only recompiled row must be the revealed row 4 at y~=60, got y={y}"
            );
        } else {
            replayed_rows += 1;
        }
    }
    assert_eq!(fresh_rows, 1, "exactly the revealed dirty row re-emits");
    assert!(
        replayed_rows >= 4,
        "the rows that merely moved must replay, got {replayed_rows} replays"
    );
}
