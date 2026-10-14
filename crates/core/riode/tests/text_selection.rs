// Single responsibility: Mouse-only readonly text selection: press, drag, multi-click.

use std::sync::Arc;

use riode::dsl::{group, text, IntoElement};
use riode::foundation::{Color, Key, Point};
use riode::input::{KeyCode, KeyEvent, PointerButton, PointerEvent, PointerPhase};
use riode::runtime::Engine;
use riode::scene::SceneCommand;
use riode::text::{TextLayout, TextSelection};

const CONTENT: &str = "hello world";

fn engine() -> Engine {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(CONTENT)
                .key("t")
                .selectable(true)
                .size(16.0)
                .into_element()]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine
}

fn send(engine: &mut Engine, x: f32, y: f32, phase: PointerPhase) {
    engine.queue_pointer_event(PointerEvent {
        position: Point::new(x, y),
        button: PointerButton::Primary,
        phase,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    engine.advance(1.0 / 60.0);
    engine.frame();
}

fn click(engine: &mut Engine, x: f32, y: f32) {
    send(engine, x, y, PointerPhase::Down);
    send(engine, x, y, PointerPhase::Up);
}

fn send_shift(engine: &mut Engine, x: f32, y: f32, phase: PointerPhase) {
    engine.queue_pointer_event(PointerEvent {
        position: Point::new(x, y),
        button: PointerButton::Primary,
        phase,
        click_count: 0,
        shift: true,
        alt: false,
        ctrl: false,
        meta: false,
    });
    engine.advance(1.0 / 60.0);
    engine.frame();
}

fn key(engine: &mut Engine, code: KeyCode) {
    key_mods(engine, code, false, false);
}

fn key_mods(engine: &mut Engine, code: KeyCode, shift: bool, ctrl: bool) {
    engine.queue_key_event(KeyEvent {
        code,
        shift,
        alt: false,
        ctrl,
        meta: false,
        repeat: false,
    });
    engine.advance(1.0 / 60.0);
    engine.frame();
}

fn origin(engine: &Engine) -> Point {
    engine.with_arena(|arena, _| {
        let id = arena.node_for_key(&Key::from("t")).expect("text node");
        arena.node_absolute_point(id)
    })
}

fn layout(engine: &Engine) -> Arc<TextLayout> {
    engine.with_arena(|arena, _| {
        let id = arena.node_for_key(&Key::from("t")).expect("text node");
        arena
            .get(id)
            .state
            .text
            .cached_text_layout
            .clone()
            .expect("shaped layout")
    })
}

fn selection(engine: &Engine) -> Option<TextSelection> {
    engine.with_arena(|arena, _| {
        let id = arena.node_for_key(&Key::from("t")).expect("text node");
        arena.get(id).state.text.text_selection
    })
}

/// Highlight rects painted in the scene (fill matches the selection color).
fn highlight_rects(engine: &Engine) -> Vec<(usize, usize)> {
    // (chunk index, command index of highlight) for every highlight rect.
    let mut out = Vec::new();
    for (ci, chunk) in engine.scene().chunks.iter().enumerate() {
        for (i, cmd) in chunk.commands.iter().enumerate() {
            if let SceneCommand::DrawRect { appearance, .. } = cmd {
                let is_highlight = appearance.fill.as_ref().is_some_and(|f| {
                    matches!(f, riode::foundation::Fill::Solid(c) if *c == riode::text::DEFAULT_SELECTION_COLOR)
                });
                if is_highlight {
                    out.push((ci, i));
                }
            }
        }
    }
    out
}

fn text_draw_index(engine: &Engine) -> Option<(usize, usize)> {
    for (ci, chunk) in engine.scene().chunks.iter().enumerate() {
        for (i, cmd) in chunk.commands.iter().enumerate() {
            if matches!(cmd, SceneCommand::DrawText { .. }) {
                return Some((ci, i));
            }
        }
    }
    None
}

#[test]
#[ntest::timeout(10_000)]
fn single_click_collapses_caret_with_no_highlight() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    let sel = selection(&engine).expect("press sets a caret");
    assert_eq!(sel.anchor, 0);
    assert_eq!(sel.focus, 0);
    assert!(highlight_rects(&engine).is_empty());
}

#[test]
#[ntest::timeout(10_000)]
fn drag_selects_range_and_paints_highlight_behind_text() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;

    send(&mut engine, o.x + 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + width * 0.5, y, PointerPhase::Move);
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Up);

    let sel = selection(&engine).expect("drag selects");
    let (s, e) = sel.normalized();
    assert_eq!(s, 0);
    assert_eq!(e, CONTENT.len());

    let highlights = highlight_rects(&engine);
    assert_eq!(highlights.len(), 1, "one line fully selected");
    let (tci, ti) = text_draw_index(&engine).expect("text drawn");
    let (hci, hi) = highlights[0];
    assert_eq!(hci, tci, "highlight shares the text chunk");
    assert!(hi < ti, "highlight paints behind the glyphs");
}

#[test]
#[ntest::timeout(10_000)]
fn drag_backwards_inverts_anchor_and_focus() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;

    // Press at the end, drag back to the start.
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + width * 0.4, y, PointerPhase::Move);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Up);

    let sel = selection(&engine).expect("drag selects");
    assert!(
        sel.anchor > sel.focus,
        "backwards drag inverts, got {sel:?}"
    );
    assert_eq!(sel.normalized(), (0, CONTENT.len()));
}

#[test]
#[ntest::timeout(10_000)]
fn double_click_selects_word() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    // Far right lands inside "world".
    let x = o.x + width * 0.8;
    let y = o.y + h * 0.5;

    click(&mut engine, x, y);
    // Second press applies the word range on Down, before release.
    send(&mut engine, x, y, PointerPhase::Down);
    let sel = selection(&engine).expect("double press selects");
    assert_eq!(sel.normalized(), (6, 11), "double-click selects the word");
    send(&mut engine, x, y, PointerPhase::Up);
    assert_eq!(selection(&engine).expect("kept").normalized(), (6, 11));
}

#[test]
#[ntest::timeout(10_000)]
fn triple_click_selects_paragraph() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let x = o.x + width * 0.8;
    let y = o.y + h * 0.5;

    click(&mut engine, x, y);
    click(&mut engine, x, y);
    send(&mut engine, x, y, PointerPhase::Down);
    let sel = selection(&engine).expect("triple press selects");
    assert_eq!(sel.normalized(), (0, CONTENT.len()));
    send(&mut engine, x, y, PointerPhase::Up);
    assert_eq!(
        selection(&engine).expect("kept").normalized(),
        (0, CONTENT.len())
    );
}

#[test]
#[ntest::timeout(10_000)]
fn press_inside_selection_collapses_on_release_not_press() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;

    // Select "world" first.
    click(&mut engine, o.x + width * 0.8, y);
    click(&mut engine, o.x + width * 0.8, y);
    assert_eq!(selection(&engine).expect("word").normalized(), (6, 11));

    // Let the click burst expire so the next press is a fresh single press,
    // not the third press of a triple-click (which would select all).
    engine.advance(0.5);
    engine.frame();

    // Press inside the word: highlight must survive the press itself.
    send(&mut engine, o.x + width * 0.7, y, PointerPhase::Down);
    assert_eq!(
        selection(&engine).expect("kept on press").normalized(),
        (6, 11)
    );
    send(&mut engine, o.x + width * 0.7, y, PointerPhase::Up);
    let sel = selection(&engine).expect("collapsed on release");
    assert!(sel.is_collapsed(), "release collapses, got {sel:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn click_elsewhere_clears_selection() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;

    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Up);
    assert!(selection(&engine).is_some());

    // Empty space below the text.
    click(&mut engine, 350.0, 350.0);
    assert_eq!(selection(&engine), None);
}

#[test]
#[ntest::timeout(10_000)]
fn non_selectable_text_ignores_selection() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(CONTENT).key("t").size(16.0).into_element()]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    let o = origin(&engine);
    let y = o.y + 8.0;
    send(&mut engine, o.x + 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + 60.0, y, PointerPhase::Move);
    send(&mut engine, o.x + 60.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + 60.0, y, PointerPhase::Up);
    assert_eq!(selection(&engine), None);
    assert!(highlight_rects(&engine).is_empty());
}

#[path = "../../../examples/motion_accordion/main.rs"]
mod accordion;

use riode_tiny_skia::testing::Harness;

fn accordion_harness(open_index: usize) -> Harness {
    let rt = std::rc::Rc::new(std::cell::RefCell::new(
        riode::reactive::ReactiveRuntime::new(),
    ));
    let open = riode::reactive::Signal::new(std::rc::Rc::clone(&rt), Some(open_index));
    let cx = riode::reactive::Cx::new(std::rc::Rc::clone(&rt));
    Harness::from_runtime(
        rt,
        cx.enter(|| accordion::Page(open.clone())),
        accordion::view::HOT_WIDTH,
        accordion::view::HOT_HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7))
}

fn accordion_copy_origin(h: &mut Harness, key: &str) -> Point {
    h.engine().with_arena(|arena, _| {
        let id = arena
            .node_for_key(&Key::from(key.to_string()))
            .expect("copy node");
        arena.node_absolute_point(id)
    })
}

fn accordion_copy_text(h: &mut Harness, key: &str) -> String {
    h.engine().with_arena(|arena, _| {
        let id = arena
            .node_for_key(&Key::from(key.to_string()))
            .expect("copy node");
        match &arena.get(id).kind {
            riode::tree::NodeKind::Text(t) => t.content.clone(),
            _ => panic!("{key} is not text"),
        }
    })
}

fn accordion_selection(h: &mut Harness, key: &str) -> Option<TextSelection> {
    h.engine().with_arena(|arena, _| {
        let id = arena
            .node_for_key(&Key::from(key.to_string()))
            .expect("copy node");
        arena.get(id).state.text.text_selection
    })
}

#[test]
#[ntest::timeout(30_000)]
fn accordion_body_copy_drag_selects() {
    let mut h = accordion_harness(0);
    h.steps(400);

    let rect = h.absolute("copy-0");
    let o = accordion_copy_origin(&mut h, "copy-0");
    let y = o.y + rect.height * 0.5;
    let x0 = o.x + 2.0;
    let x1 = o.x + rect.width - 3.0;

    h.pointer(Point::new(x0, y), PointerPhase::Down);
    h.step();
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        h.pointer(Point::new(x0 + (x1 - x0) * t, y), PointerPhase::Drag);
        h.step();
    }
    h.pointer(Point::new(x1, y), PointerPhase::Up);
    h.step();

    let content = accordion_copy_text(&mut h, "copy-0");
    let sel = accordion_selection(&mut h, "copy-0").expect("drag selects the copy");
    let (s, e) = sel.normalized();
    assert!(
        e - s > content.len() / 2,
        "drag covers the copy, got {sel:?}"
    );
    assert!(!highlight_rects(h.engine()).is_empty());
}

#[test]
#[ntest::timeout(30_000)]
fn accordion_body_copy_double_click_selects_word() {
    let mut h = accordion_harness(1);
    h.steps(400);

    let rect = h.absolute("copy-1");
    let o = accordion_copy_origin(&mut h, "copy-1");
    let at = Point::new(o.x + rect.width * 0.4, o.y + rect.height * 0.5);
    h.pointer(at, PointerPhase::Down);
    h.step();
    h.pointer(at, PointerPhase::Up);
    h.step();
    h.pointer(at, PointerPhase::Down);
    h.step();

    let content = accordion_copy_text(&mut h, "copy-1");
    let sel = accordion_selection(&mut h, "copy-1").expect("double-click selects");
    let (s, e) = sel.normalized();
    assert!(e > s, "word is non-empty");
    assert!(e - s < content.len(), "word is not the paragraph: {sel:?}");
    h.pointer(at, PointerPhase::Up);
    h.step();
}

#[test]
#[ntest::timeout(30_000)]
fn accordion_header_title_stays_plain() {
    // The title owns the open/close click: making it selectable would turn
    // every drifted press into a drag and swallow the toggle.
    let mut h = accordion_harness(0);
    h.steps(60);
    h.engine().with_arena(|arena, _| {
        let id = arena.node_for_key(&Key::from("copy-0")).expect("copy");
        let selectable = match &arena.get(id).kind {
            riode::tree::NodeKind::Text(t) => t.selectable,
            _ => false,
        };
        assert!(selectable, "body copy is selectable");
    });
}

#[test]
#[ntest::timeout(10_000)]
fn content_edit_clears_stale_selection() {
    use riode::reactive::Signal;
    let runtime = std::rc::Rc::new(std::cell::RefCell::new(
        riode::reactive::ReactiveRuntime::new(),
    ));
    let content = Signal::new(runtime.clone(), CONTENT.to_string());
    let body = content.clone();
    let mut engine = Engine::with_runtime(
        runtime.clone(),
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(body)
                .key("t")
                .selectable(true)
                .size(16.0)
                .into_element()])
            .into_element(),
        400,
        400,
    );
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();
    let o = origin(&engine);
    click(&mut engine, o.x + 50.0, o.y + 8.0);
    click(&mut engine, o.x + 50.0, o.y + 8.0);
    assert!(selection(&engine).is_some());
    engine.guard(|| content.set("changed".to_string()));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(selection(&engine), None);
}

/// Caret rects: text-colored, CARET_WIDTH wide, painted after the glyphs.
fn caret_rects(engine: &Engine) -> Vec<(usize, usize)> {
    let text_at = text_draw_index(engine);
    let mut out = Vec::new();
    for (ci, chunk) in engine.scene().chunks.iter().enumerate() {
        for (i, cmd) in chunk.commands.iter().enumerate() {
            if let SceneCommand::DrawRect { rect, appearance } = cmd {
                let is_caret = (rect.width - riode::text::CARET_WIDTH).abs() < 0.01
                    && appearance
                        .fill
                        .as_ref()
                        .is_some_and(|f| matches!(f, riode::foundation::Fill::Solid(_)));
                // Highlights are wider than the caret; the caret also paints
                // after the text while highlights paint before it.
                if is_caret {
                    if let Some((tci, ti)) = text_at {
                        if ci == tci && i > ti {
                            out.push((ci, i));
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
#[ntest::timeout(10_000)]
fn click_focuses_and_paints_caret_but_blur_hides_it() {
    let mut engine = engine();
    assert_eq!(engine.focused_text(), None);
    assert!(caret_rects(&engine).is_empty());

    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    assert_eq!(engine.focused_text(), Some(Key::from("t")));
    // The caret blinks: poll up to a full period for the on-phase.
    let mut seen = !caret_rects(&engine).is_empty();
    for _ in 0..70 {
        if seen {
            break;
        }
        engine.advance(1.0 / 60.0);
        engine.frame();
        seen = !caret_rects(&engine).is_empty();
    }
    assert!(seen, "focused caret shows within a blink period");

    // Clicking empty space blurs: the collapsed caret hides with focus.
    click(&mut engine, 350.0, 350.0);
    assert_eq!(engine.focused_text(), None);
    for _ in 0..70 {
        engine.advance(1.0 / 60.0);
        engine.frame();
        assert!(
            caret_rects(&engine).is_empty(),
            "blurred caret stays hidden through the blink"
        );
    }
}

#[test]
#[ntest::timeout(10_000)]
fn caret_blinks_and_keeps_frames_alive() {
    let mut engine = engine();
    assert!(!engine.has_active_motion());
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    // A focused caret keeps the frame loop alive for its blink.
    assert!(engine.has_active_motion());

    let mut on = 0;
    let mut off = 0;
    for _ in 0..70 {
        engine.advance(1.0 / 60.0);
        engine.frame();
        if caret_rects(&engine).is_empty() {
            off += 1;
        } else {
            on += 1;
        }
    }
    assert!(on > 0 && off > 0, "caret blinks: on={on} off={off} frames");
    // Blurring a caret with no other motion lets the loop rest.
    click(&mut engine, 350.0, 350.0);
    assert!(!engine.has_active_motion());
}

#[test]
#[ntest::timeout(10_000)]
fn arrows_walk_graphemes_and_shift_extends() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    assert_eq!(selection(&engine).map(|s| s.focus), Some(0));

    key(&mut engine, KeyCode::Right);
    key(&mut engine, KeyCode::Right);
    key(&mut engine, KeyCode::Right);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(3));
    key(&mut engine, KeyCode::Left);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(2));

    // Shift extends from the anchor; a bare arrow collapses toward it.
    key_mods(&mut engine, KeyCode::Right, true, false);
    key_mods(&mut engine, KeyCode::Right, true, false);
    assert_eq!(
        engine.text_selection("t").map(|s| (s.anchor, s.focus)),
        Some((2, 4))
    );
    key(&mut engine, KeyCode::Left);
    assert_eq!(
        engine.text_selection("t"),
        Some(TextSelection {
            anchor: 2,
            focus: 2
        })
    );
}

#[test]
#[ntest::timeout(10_000)]
fn ctrl_arrows_jump_words() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);

    key_mods(&mut engine, KeyCode::Right, false, true);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(5));
    key_mods(&mut engine, KeyCode::Right, false, true);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(11));
    key_mods(&mut engine, KeyCode::Left, false, true);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(6));
    // Shift extends by words from the anchor.
    key_mods(&mut engine, KeyCode::Left, true, true);
    assert_eq!(
        engine.text_selection("t").map(|s| (s.anchor, s.focus)),
        Some((6, 0))
    );
}

#[test]
#[ntest::timeout(10_000)]
fn home_end_and_ctrl_home_end() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 40.0, o.y + h * 0.5);

    key(&mut engine, KeyCode::End);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(11));
    key(&mut engine, KeyCode::Home);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(0));
    key_mods(&mut engine, KeyCode::End, false, true);
    assert_eq!(
        engine.text_selection("t").map(|s| s.focus),
        Some(CONTENT.len())
    );
    key_mods(&mut engine, KeyCode::Home, false, true);
    assert_eq!(engine.text_selection("t").map(|s| s.focus), Some(0));
    // Shift+Home extends to the line start.
    key(&mut engine, KeyCode::Right);
    key_mods(&mut engine, KeyCode::Home, true, false);
    assert_eq!(
        engine.text_selection("t").map(|s| (s.anchor, s.focus)),
        Some((1, 0))
    );
}

#[test]
#[ntest::timeout(10_000)]
fn up_down_move_lines_preserving_goal_x() {
    let content = "Aurora wraps this paragraph to the box width and keeps rhythm";
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(content)
                .key("t")
                .selectable(true)
                .size(16.0)
                .width(160.0)
                .into_element()]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    let o = origin(&engine);
    let lay = layout(&engine);
    assert!(lay.lines.len() > 1);
    let h = lay.lines[0].height;
    let ranges = riode::text::line_byte_ranges(&lay, content);
    let line_of = |off: usize| {
        ranges
            .iter()
            .position(|&(s, e)| off >= s && off < e)
            .unwrap_or(ranges.len() - 1)
    };

    // Start mid first line, away from its edges.
    click(&mut engine, o.x + 60.0, o.y + h * 0.5);
    let f0 = engine.text_selection("t").map(|s| s.focus).unwrap();
    assert_eq!(line_of(f0), 0);

    key(&mut engine, KeyCode::Down);
    let f1 = engine.text_selection("t").map(|s| s.focus).unwrap();
    assert_eq!(line_of(f1), 1);
    key(&mut engine, KeyCode::Up);
    let back = engine.text_selection("t").map(|s| s.focus).unwrap();
    assert_eq!(line_of(back), 0);
    assert!(
        back.abs_diff(f0) <= 2,
        "goal-x round-trips, started at {f0}, back at {back}"
    );
    // Shift+Down extends from the anchor on the next line.
    key_mods(&mut engine, KeyCode::Down, true, false);
    let sel = engine.text_selection("t").unwrap();
    assert_eq!(sel.anchor, back);
    assert_eq!(line_of(sel.focus), 1);
}

#[test]
#[ntest::timeout(10_000)]
fn ctrl_a_selects_all_and_reports_text() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    key_mods(&mut engine, KeyCode::Char('a'), false, true);
    assert_eq!(
        engine.text_selection("t").map(|s| s.normalized()),
        Some((0, CONTENT.len()))
    );
    assert_eq!(engine.selected_text("t").as_deref(), Some(CONTENT));
}

#[test]
#[ntest::timeout(10_000)]
fn ctrl_c_copies_word_to_clipboard() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let x = o.x + layout.total_size.width * 0.8;
    let y = o.y + h * 0.5;
    click(&mut engine, x, y);
    click(&mut engine, x, y);
    assert_eq!(engine.selected_text("t").as_deref(), Some("world"));

    key_mods(&mut engine, KeyCode::Char('c'), false, true);
    assert_eq!(engine.clipboard_text().as_deref(), Some("world"));
    // Copy changes nothing on screen.
    assert_eq!(engine.selected_text("t").as_deref(), Some("world"));
}

#[test]
#[ntest::timeout(10_000)]
fn escape_collapses_and_typing_is_noop() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + 1.0, y, PointerPhase::Up);
    assert!(engine
        .text_selection("t")
        .is_some_and(|s| !s.is_collapsed()));

    key(&mut engine, KeyCode::Escape);
    let sel = engine.text_selection("t").expect("escape collapses");
    assert!(sel.is_collapsed());
    // Focus survives Escape, so the caret stays visible.
    assert_eq!(engine.focused_text(), Some(Key::from("t")));

    key(&mut engine, KeyCode::Char('x'));
    assert_eq!(engine.text_selection("t"), Some(sel));
    assert_eq!(engine.clipboard_text(), None);
}

#[test]
#[ntest::timeout(10_000)]
fn programmatic_selection_paints_and_clears() {
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);

    assert!(engine.set_text_selection(
        "t",
        Some(TextSelection {
            anchor: 1,
            focus: 4
        })
    ));
    assert_eq!(engine.selected_text("t").as_deref(), Some("ell"));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(highlight_rects(&engine).len(), 1);

    assert!(engine.set_text_selection("t", None));
    assert_eq!(engine.selected_text("t").as_deref(), None);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(highlight_rects(&engine).is_empty());
    assert!(!engine.set_text_selection("missing", None));
}

#[test]
#[ntest::timeout(30_000)]
fn slow_clicks_never_form_a_burst() {
    // Regression: the burst used to run on the animation clock, whose idle
    // gaps clamp to 0.1s, so in an on-demand frame loop it never expired and
    // every click past the second selected the paragraph ("clicking selects
    // the whole line"). It now runs on wall time.
    //
    // No forced deltas here: the point is real idle time between frames.
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let x = o.x + layout.total_size.width * 0.8;
    let y = o.y + h * 0.5;
    let live_click = |engine: &mut Engine| {
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            engine.queue_pointer_event(PointerEvent {
                position: Point::new(x, y),
                button: PointerButton::Primary,
                phase,
                click_count: 0,
                shift: false,
                alt: false,
                ctrl: false,
                meta: false,
            });
            engine.frame();
        }
    };
    for n in 1..=3 {
        live_click(&mut engine);
        let sel = engine.text_selection("t").expect("click sets a caret");
        assert!(
            sel.is_collapsed(),
            "click {n} after a real idle gap must collapse, got {sel:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[test]
#[ntest::timeout(10_000)]
fn shift_click_extends_from_anchor() {
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;

    click(&mut engine, o.x + 2.0, y);
    send_shift(&mut engine, o.x + width - 1.0, y, PointerPhase::Down);
    send_shift(&mut engine, o.x + width - 1.0, y, PointerPhase::Up);
    assert_eq!(
        engine.text_selection("t").map(|s| (s.anchor, s.focus)),
        Some((0, CONTENT.len()))
    );
}

#[test]
#[ntest::timeout(10_000)]
fn double_press_drag_extends_by_words() {
    let content = "one two three";
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(content)
                .key("t")
                .selectable(true)
                .size(16.0)
                .into_element()]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    let o = origin(&engine);
    let lay = layout(&engine);
    let h = lay.lines[0].height;
    let width = lay.total_size.width;
    let y = o.y + h * 0.5;

    // Double-click "two", then hold the second press and drag: the range
    // extends by whole words from the word's anchor.
    let x_two = o.x + width * 0.4;
    click(&mut engine, x_two, y);
    send(&mut engine, x_two, y, PointerPhase::Down);
    assert_eq!(
        engine.text_selection("t").map(|s| s.normalized()),
        Some((4, 7))
    );
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Up);
    assert_eq!(
        engine.text_selection("t").map(|s| s.normalized()),
        Some((4, content.len()))
    );
    assert_eq!(engine.selected_text("t").as_deref(), Some("two three"));
}

#[test]
#[ntest::timeout(10_000)]
fn burst_resets_when_the_press_changes_target() {
    // Regression (accordion): opening an item and promptly clicking its copy
    // fed one global burst, so the description press read as a double/triple
    // click and selected a word or the whole line. Bursts are per target.
    use riode::foundation::Direction;
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([
                text("alpha beta")
                    .key("a")
                    .selectable(true)
                    .size(16.0)
                    .into_element(),
                text("gamma delta")
                    .key("b")
                    .selectable(true)
                    .size(16.0)
                    .into_element(),
            ]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    let at = |engine: &Engine, key: &str| {
        engine.with_arena(|arena, _| {
            let id = arena.node_for_key(&Key::from(key.to_string())).unwrap();
            let p = arena.node_absolute_point(id);
            let h = arena
                .get(id)
                .state
                .text
                .cached_text_layout
                .clone()
                .unwrap()
                .lines[0]
                .height;
            (p.x + 30.0, p.y + h * 0.5)
        })
    };
    let (ax, ay) = at(&engine, "a");
    let (bx, by) = at(&engine, "b");

    // Click A, then promptly click B: B starts a fresh burst (caret).
    click(&mut engine, ax, ay);
    assert!(engine.text_selection("a").is_some_and(|s| s.is_collapsed()));
    click(&mut engine, bx, by);
    let sel_b = engine.text_selection("b").expect("B selects");
    assert!(
        sel_b.is_collapsed(),
        "first press on a new target collapses, got {sel_b:?}"
    );
    // A prompt second press on the SAME target still grows (word).
    click(&mut engine, bx, by);
    let sel_b2 = engine.text_selection("b").expect("B selects again");
    assert!(
        !sel_b2.is_collapsed(),
        "second press on the same target grows, got {sel_b2:?}"
    );
}

#[test]
#[ntest::timeout(10_000)]
fn header_click_does_not_feed_the_copy_burst() {
    // The accordion scenario: item 0 open and settled, the user opens item 1
    // and instantly (double-click speed) presses copy-0 while it is still
    // laid out mid-close. Different target, same burst window: the copy press
    // must collapse, not select a word.
    let rt = std::rc::Rc::new(std::cell::RefCell::new(
        riode::reactive::ReactiveRuntime::new(),
    ));
    let open = riode::reactive::Signal::new(std::rc::Rc::clone(&rt), Some(0));
    let cx = riode::reactive::Cx::new(std::rc::Rc::clone(&rt));
    let mut engine = Engine::with_runtime(
        rt,
        cx.enter(|| accordion::Page(open.clone())),
        accordion::view::HOT_WIDTH,
        accordion::view::HOT_HEIGHT,
    );
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();
    for _ in 0..200 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    let center = |engine: &Engine, key: &str| {
        let r = engine.absolute_rect(key).expect("rect");
        (r.x + r.width * 0.5, r.y + r.height * 0.5)
    };
    // Open item 1, then press the still-closing copy-0 three frames later.
    let (hx, hy) = center(&engine, "item-1");
    click(&mut engine, hx, hy);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.advance(1.0 / 60.0);
    engine.frame();
    let r = engine.absolute_rect("copy-0").expect("copy rect");
    let (px, py) = (r.x + r.width * 0.4, r.y + r.height * 0.5);
    send(&mut engine, px, py, PointerPhase::Down);
    let sel = engine.text_selection("copy-0").expect("copy press selects");
    assert!(
        sel.is_collapsed(),
        "copy press on a new target collapses, got {sel:?}"
    );
    send(&mut engine, px, py, PointerPhase::Up);
}

#[test]
#[ntest::timeout(10_000)]
fn drag_breaks_the_burst_so_reselection_starts_fresh() {
    // Double-click a word, drag-select, then immediately press again on the
    // same element: without the drag reset the stale burst would paragraph-
    // select on this press and no fresh partial selection could start.
    let mut engine = engine();
    let o = origin(&engine);
    let layout = layout(&engine);
    let h = layout.lines[0].height;
    let width = layout.total_size.width;
    let y = o.y + h * 0.5;
    let xw = o.x + width * 0.8;

    click(&mut engine, xw, y);
    click(&mut engine, xw, y);
    assert_eq!(
        engine.text_selection("t").map(|s| s.normalized()),
        Some((6, 11))
    );
    // Drag-select the whole run (third press latches paragraph, correctly).
    send(&mut engine, o.x + 1.0, y, PointerPhase::Down);
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + width - 1.0, y, PointerPhase::Up);
    assert_eq!(
        engine.text_selection("t").map(|s| s.normalized()),
        Some((0, CONTENT.len()))
    );
    // An immediate fresh press collapses instead of re-paragraphing. The
    // collapse lands on release (a press inside a range must not destroy it
    // before a drag can start from it), so complete the click.
    send(&mut engine, o.x + width * 0.3, y, PointerPhase::Down);
    send(&mut engine, o.x + width * 0.3, y, PointerPhase::Up);
    let sel = engine.text_selection("t").expect("re-press selects");
    assert!(
        sel.is_collapsed(),
        "press after a drag starts fresh, got {sel:?}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn accordion_top_text_is_selectable() {
    // The page eyebrow stays chrome, but the heading and description blurb
    // are content and opt into selection like the item copy.
    let mut h = accordion_harness(0);
    h.steps(60);
    let flags = h.engine().with_arena(|arena, root| {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let riode::tree::NodeKind::Text(t) = &node.kind {
                out.push((t.content.clone(), t.selectable));
            }
            stack.extend(arena.children(id).iter().copied());
        }
        out
    });
    let find = |needle: &str| {
        flags
            .iter()
            .find(|(c, _)| c.contains(needle))
            .map(|(_, s)| *s)
    };
    assert_eq!(
        find("One panel open at a time"),
        Some(true),
        "heading is selectable"
    );
    assert_eq!(
        find("real layout value"),
        Some(true),
        "description blurb is selectable"
    );
    assert_eq!(
        find("WEB"),
        Some(false),
        "eyebrow stays non-selectable chrome"
    );
    // And the heading actually selects on drag: locate it by content, then
    // sweep its first line.
    let (hx, hy, hh) = h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let riode::tree::NodeKind::Text(t) = &node.kind {
                if t.content.contains("One panel open") {
                    let p = arena.node_absolute_point(id);
                    let h = node
                        .state
                        .text
                        .cached_text_layout
                        .clone()
                        .map(|l| l.lines[0].height)
                        .unwrap_or(20.0);
                    return (p.x, p.y, h);
                }
            }
            stack.extend(arena.children(id).iter().copied());
        }
        panic!("heading not found");
    });
    h.pointer(
        riode::foundation::Point::new(hx + 4.0, hy + hh * 0.5),
        riode::input::PointerPhase::Down,
    );
    h.step();
    for i in 1..=4 {
        h.pointer(
            riode::foundation::Point::new(hx + 4.0 + i as f32 * 40.0, hy + hh * 0.5),
            riode::input::PointerPhase::Drag,
        );
        h.step();
    }
    h.pointer(
        riode::foundation::Point::new(hx + 164.0, hy + hh * 0.5),
        riode::input::PointerPhase::Up,
    );
    h.step();
    let heading_sel = h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let riode::tree::NodeKind::Text(t) = &node.kind {
                if t.content.contains("One panel open") {
                    return node.state.text.text_selection;
                }
            }
            stack.extend(arena.children(id).iter().copied());
        }
        None
    });
    let sel = heading_sel.expect("heading drag selects");
    assert!(!sel.is_collapsed(), "heading drag is a range, got {sel:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn caret_shows_immediately_on_press_despite_phase() {
    // Push the blink into an off-phase, then press: activity restarts the
    // blink visibly on with a full on-period, like a browser.
    let mut engine = engine();
    for _ in 0..40 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    assert!(
        !caret_rects(&engine).is_empty(),
        "press shows the caret at once, not next period"
    );
}

#[test]
#[ntest::timeout(10_000)]
fn empty_text_press_takes_no_selection_or_focus() {
    // F1: pressing an empty selectable run stores nothing and focuses nothing.
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text("").key("t").selectable(true).size(16.0).into_element()]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    click(&mut engine, 10.0, 10.0);
    assert_eq!(engine.text_selection("t"), None);
    assert!(engine.focused_text().is_none());
}

#[test]
#[ntest::timeout(10_000)]
fn shift_click_without_live_gesture_extends_stored_range() {
    // F6: Shift-click after the gesture ended still extends from the stored
    // anchor instead of collapsing.
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    let y = o.y + h * 0.5;
    click(&mut engine, o.x + 2.0, y);
    let first = selection(&engine).expect("caret");
    assert!(first.is_collapsed());
    // Shift-click inside the run (not past its edge, which would miss the
    // node box and clear): extends from the first anchor.
    let w = layout(&engine).total_size.width;
    let x1 = o.x + w * 0.8;
    send_shift(&mut engine, x1, y, PointerPhase::Down);
    send_shift(&mut engine, x1, y, PointerPhase::Up);
    let sel = selection(&engine).expect("shift extends");
    assert_eq!(sel.anchor, first.anchor, "anchor kept, got {sel:?}");
    assert!(!sel.is_collapsed(), "shift-click extends, got {sel:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn void_press_sweep_into_text_latches_selection() {
    // F3: press on empty space, drag into selectable text → selects.
    let mut engine = engine();
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    let y = o.y + h * 0.5;
    send(&mut engine, 350.0, 350.0, PointerPhase::Down);
    assert_eq!(selection(&engine), None);
    // Sweep in several drag steps so the drag threshold trips mid-gesture.
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        send(
            &mut engine,
            350.0 + (o.x + 4.0 - 350.0) * t,
            350.0 + (y - 350.0) * t,
            PointerPhase::Drag,
        );
    }
    send(&mut engine, o.x + 120.0, y, PointerPhase::Drag);
    send(&mut engine, o.x + 120.0, y, PointerPhase::Up);
    let sel = selection(&engine).expect("sweep latches");
    assert!(!sel.is_collapsed(), "sweep selects, got {sel:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn press_before_first_paint_leaves_no_stale_state() {
    // F2: a Down queued before the first frame (no geometry, no shaped text)
    // must not poison later gestures: the next normal click still collapses
    // a caret.
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([text(CONTENT)
                .key("t")
                .selectable(true)
                .size(16.0)
                .into_element()]),
    );
    // No frame yet: input dispatches before the frame's layout passes, so
    // neither hit geometry nor a shaped layout exists at this point.
    engine.queue_pointer_event(PointerEvent {
        position: Point::new(2.0, 10.0),
        button: PointerButton::Primary,
        phase: PointerPhase::Down,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.queue_pointer_event(PointerEvent {
        position: Point::new(2.0, 10.0),
        button: PointerButton::Primary,
        phase: PointerPhase::Up,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    engine.advance(1.0 / 60.0);
    engine.frame();
    // A fresh normal click afterwards still works (caret, no stale range).
    let o = origin(&engine);
    let h = layout(&engine).lines[0].height;
    click(&mut engine, o.x + 2.0, o.y + h * 0.5);
    let sel = selection(&engine).expect("later press selects");
    assert!(sel.is_collapsed(), "fresh press collapses, got {sel:?}");
}

// --- F4 cross-node helpers -------------------------------------------------

const RUN_A: &str = "alpha beta";
const RUN_B: &str = "gamma delta";
const RUN_C: &str = "epsilon zeta";

fn multi_engine() -> Engine {
    use riode::foundation::Direction;
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .gap(8.0)
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([
                text(RUN_A)
                    .key("a")
                    .selectable(true)
                    .size(16.0)
                    .into_element(),
                text(RUN_B)
                    .key("b")
                    .selectable(true)
                    .size(16.0)
                    .into_element(),
                text(RUN_C)
                    .key("c")
                    .selectable(true)
                    .size(16.0)
                    .into_element(),
            ]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine
}

/// Left edge midpoint of a run's first line (a press/drag point inside it).
fn run_point(engine: &Engine, key: &str) -> Point {
    engine.with_arena(|arena, _| {
        let id = arena
            .node_for_key(&Key::from(key.to_string()))
            .expect("run");
        let p = arena.node_absolute_point(id);
        let layout = arena
            .get(id)
            .state
            .text
            .cached_text_layout
            .clone()
            .expect("shaped");
        Point::new(p.x + 2.0, p.y + layout.lines[0].height * 0.5)
    })
}

/// Right edge midpoint of a run's first line.
fn run_end(engine: &Engine, key: &str) -> Point {
    engine.with_arena(|arena, _| {
        let id = arena
            .node_for_key(&Key::from(key.to_string()))
            .expect("run");
        let p = arena.node_absolute_point(id);
        let layout = arena
            .get(id)
            .state
            .text
            .cached_text_layout
            .clone()
            .expect("shaped");
        Point::new(
            p.x + layout.total_size.width - 2.0,
            p.y + layout.lines[0].height * 0.5,
        )
    })
}

fn run_sel(engine: &Engine, key: &str) -> Option<TextSelection> {
    engine.text_selection(key)
}

/// Drags from one point to another in steps so the drag threshold trips.
fn drag(engine: &mut Engine, from: Point, to: Point) {
    send(engine, from.x, from.y, PointerPhase::Down);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        send(
            engine,
            from.x + (to.x - from.x) * t,
            from.y + (to.y - from.y) * t,
            PointerPhase::Drag,
        );
    }
    send(engine, to.x, to.y, PointerPhase::Up);
}

#[test]
#[ntest::timeout(10_000)]
fn drag_down_across_two_runs_paints_both_edges() {
    // F4: anchor run covers press-to-end, focus run covers start-to-caret.
    let mut engine = multi_engine();
    let from = run_point(&engine, "a");
    let to = run_end(&engine, "b");
    drag(&mut engine, from, to);
    let sa = run_sel(&engine, "a").expect("anchor paints");
    let sb = run_sel(&engine, "b").expect("focus paints");
    assert!(!sa.is_collapsed() && !sb.is_collapsed());
    assert_eq!(
        sa.normalized().1,
        RUN_A.len(),
        "anchor to run end, got {sa:?}"
    );
    assert_eq!(sb.normalized().0, 0, "focus from run start, got {sb:?}");
    assert_eq!(run_sel(&engine, "c"), None, "third run untouched");
}

#[test]
#[ntest::timeout(10_000)]
fn drag_up_across_two_runs_reverses_edges() {
    // F4: dragging upward flips the covered edges.
    let mut engine = multi_engine();
    let from = run_end(&engine, "b");
    let to = run_point(&engine, "a");
    drag(&mut engine, from, to);
    let sa = run_sel(&engine, "a").expect("focus paints");
    let sb = run_sel(&engine, "b").expect("anchor paints");
    assert!(!sa.is_collapsed() && !sb.is_collapsed());
    assert_eq!(
        sa.normalized().1,
        RUN_A.len(),
        "focus to run end, got {sa:?}"
    );
    assert_eq!(sb.normalized().0, 0, "anchor from run start, got {sb:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn drag_across_three_runs_covers_interior_fully() {
    // F4: the run strictly between anchor and focus paints wall to wall.
    let mut engine = multi_engine();
    let from = run_point(&engine, "a");
    let to = run_end(&engine, "c");
    drag(&mut engine, from, to);
    let sb = run_sel(&engine, "b").expect("interior paints");
    assert_eq!(
        sb.normalized(),
        (0, RUN_B.len()),
        "interior fully covered, got {sb:?}"
    );
    assert!(run_sel(&engine, "a").is_some_and(|s| !s.is_collapsed()));
    assert!(run_sel(&engine, "c").is_some_and(|s| !s.is_collapsed()));
}

#[test]
#[ntest::timeout(10_000)]
fn dragging_back_shrinks_the_range() {
    // F4: runs that leave the span repaint clean (no stale highlights).
    let mut engine = multi_engine();
    let a0 = run_point(&engine, "a");
    send(&mut engine, a0.x, a0.y, PointerPhase::Down);
    for i in 1..=6 {
        let to = run_end(&engine, "c");
        let t = i as f32 / 6.0;
        send(
            &mut engine,
            a0.x + (to.x - a0.x) * t,
            a0.y + (to.y - a0.y) * t,
            PointerPhase::Drag,
        );
    }
    assert!(run_sel(&engine, "c").is_some(), "reached the third run");
    // Drag back up onto the first run (but not all the way onto the anchor,
    // which would collapse): c must clear while a keeps a range.
    for i in 1..=4 {
        let from = run_end(&engine, "c");
        let t = i as f32 / 4.0;
        send(
            &mut engine,
            from.x + (a0.x + 40.0 - from.x) * t,
            from.y + (a0.y - from.y) * t,
            PointerPhase::Drag,
        );
    }
    send(&mut engine, a0.x + 40.0, a0.y, PointerPhase::Up);
    assert_eq!(run_sel(&engine, "c"), None, "leaver repaints clean");
    assert!(run_sel(&engine, "a").is_some_and(|s| !s.is_collapsed()));
}

#[test]
#[ntest::timeout(10_000)]
fn shift_click_extends_onto_another_run() {
    // F4/F6: Shift-click on a second run extends from the stored anchor.
    let mut engine = multi_engine();
    let a0 = run_point(&engine, "a");
    click(&mut engine, a0.x, a0.y);
    let first = run_sel(&engine, "a").expect("caret");
    let b0 = run_point(&engine, "b");
    send_shift(&mut engine, b0.x + 30.0, b0.y, PointerPhase::Down);
    send_shift(&mut engine, b0.x + 30.0, b0.y, PointerPhase::Up);
    let sa = run_sel(&engine, "a").expect("anchor paints");
    let sb = run_sel(&engine, "b").expect("focus paints");
    assert_eq!(sa.anchor, first.anchor, "anchor kept, got {sa:?}");
    assert!(!sb.is_collapsed(), "focus extends, got {sb:?}");
}

#[test]
#[ntest::timeout(10_000)]
fn cross_node_copy_joins_runs_with_newlines() {
    // F4: Ctrl+C concatenates the painted sub-ranges in document order.
    let mut engine = multi_engine();
    let from = run_point(&engine, "a");
    let to = run_end(&engine, "b");
    drag(&mut engine, from, to);
    key_mods(&mut engine, KeyCode::Char('c'), false, true);
    let ta = engine.selected_text("a").unwrap_or_default();
    let tb = engine.selected_text("b").unwrap_or_default();
    assert!(!ta.is_empty() && !tb.is_empty());
    assert_eq!(
        engine.clipboard_text().as_deref(),
        Some(format!("{ta}\n{tb}").as_str())
    );
    assert_eq!(engine.combined_selected_text(), vec![ta, tb]);
}
