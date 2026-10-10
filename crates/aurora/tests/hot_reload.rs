// Single responsibility: Hot-reload rebuilds the tree from a swapped factory
// while preserving every signal, spring, and ticker.
//
// A reload re-runs the root factory under the same stable `"root"` hook scope and
// reconciles the new element tree against the live arena. These tests pin that contract:
// a color patch lands, hook state survives, failures keep the old tree, and the debug
// commands queue the same path a file watcher or the panel button would trigger.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::Element;
use aurora::hot_reload::SharedFactory;
use aurora::prelude::*;
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora::runtime::Engine;

/// A stateful swatch: its height lives only in hook state, its fill comes from the
/// factory's captured tone (standing in for "the edited source").
#[component]
fn Swatch(tone: Rc<RefCell<u32>>, extra: bool) -> impl IntoElement {
    let taps = use_signal(0u32);
    let bump = taps.clone();
    let height = taps.clone();
    let mut root = group()
        .key("swatch")
        .width(100.0)
        .height(height.map(|n| 100.0 + n as f32 * 20.0))
        .fill(Color::hex(*tone.borrow()))
        .on_click(move |_| bump.set(bump.get() + 1));
    if extra {
        root = root.children([text("v2").size(12.0).color(Color::WHITE)]);
    }
    root
}

fn press(engine: &mut Engine, x: f32, y: f32) {
    engine.queue_pointer_down(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.queue_pointer_up(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
}

fn swatch_height(engine: &Engine) -> f32 {
    engine.rect("swatch").map(|r| r.height).unwrap_or(-1.0)
}

fn swatch_fill(engine: &Engine) -> Option<aurora::foundation::Fill> {
    engine.with_arena(|arena, _| {
        let key: aurora::foundation::Key = "swatch".into();
        arena
            .node_for_key(&key)
            .and_then(|id| arena.get(id).style.appearance.fill.clone())
    })
}

/// Mounts an engine whose root factory builds `Swatch` from the shared tone cell.
///
/// The swatch sits inside a full-viewport parent: the root node always fills the window
/// (tight constraints), so size assertions need a real parent.
///
/// Mirrors `AppConfig::run` exactly: one build window spans the factory and the engine
/// creation, so the initial tree's registrations tag with a single generation.
fn mount(tone: Rc<RefCell<u32>>, extra: Rc<RefCell<bool>>) -> (Engine, SharedFactory) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    runtime.borrow_mut().begin_build_window();
    let cx = Cx::new(Rc::clone(&runtime));
    let factory: SharedFactory = Rc::new(move || {
        group()
            .width(400.0)
            .height(400.0)
            .children([Swatch(Rc::clone(&tone), *extra.borrow())])
            .into_element()
    });
    let root: Element = cx.enter_keyed("root", || factory());
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    runtime.borrow_mut().end_build_window(true);
    engine.set_root_factory(Rc::clone(&factory));
    engine.advance(1.0 / 60.0);
    engine.frame();
    (engine, factory)
}

fn ticker_count(engine: &Engine) -> usize {
    engine.runtime().borrow().ticker_count()
}

/// A component holding frame-driven registrations: re-running its body must not
/// accumulate tickers across reloads.
#[component]
fn Ticking() -> impl IntoElement {
    use_interval(3600.0, || {});
    let _clock = use_frame_clock();
    group().key("ticking").width(10.0).height(10.0)
}

fn mount_ticking() -> Engine {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    runtime.borrow_mut().begin_build_window();
    let cx = Cx::new(Rc::clone(&runtime));
    let factory: SharedFactory = Rc::new(|| Ticking().into_element());
    let root: Element = cx.enter_keyed("root", || factory());
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    runtime.borrow_mut().end_build_window(true);
    engine.set_root_factory(Rc::clone(&factory));
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine
}

/// A component with an explicitly pinned identity, surviving file moves and renames.
#[component(name = "pinned::Card")]
fn NamedCard() -> impl IntoElement {
    let taps = use_signal(0u32);
    let bump = taps.clone();
    let height = taps.clone();
    group()
        .key("named")
        .width(50.0)
        .height(height.map(|n| 50.0 + n as f32))
        .on_click(move |_| bump.set(bump.get() + 1))
}

#[test]
#[ntest::timeout(30_000)]
fn reload_patches_color_and_preserves_hook_state() {
    let tone = Rc::new(RefCell::new(0x2563EB_u32));
    let extra = Rc::new(RefCell::new(false));
    let (mut engine, _) = mount(Rc::clone(&tone), Rc::clone(&extra));

    assert!(
        (swatch_height(&engine) - 100.0).abs() < 0.5,
        "swatch starts at base height"
    );
    press(&mut engine, 50.0, 50.0);
    assert!(
        (swatch_height(&engine) - 120.0).abs() < 0.5,
        "one tap grows the swatch from hook state"
    );

    // "Edit the source": the tone changes, then a reload rebuilds from the factory.
    *tone.borrow_mut() = 0xDC2626;
    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert_eq!(
        swatch_fill(&engine),
        Some(Fill::from(Color::hex(0xDC2626))),
        "the edited color lands on the live node"
    );
    assert!(
        (swatch_height(&engine) - 120.0).abs() < 0.5,
        "hook state survives the reload"
    );
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_total, 1, "one successful reload recorded");
    assert_eq!(stats.last_reloaded, vec!["*".to_string()]);
    assert!(stats.last_error.is_none());
    // Impact: the frame re-laid out (root remount marks layout), dirtied nodes, and
    // produced damage for the repainted swatch.
    assert!(stats.last_laid_out, "reload frame ran layout");
    assert!(stats.last_nodes_dirtied >= 1, "reload dirtied nodes");
    assert!(stats.last_damage_rects >= 1, "reload repainted");
}

/// A text color edit must repaint: `reconcile_text` marks PAINT alongside
/// measure/layout because the layout pass clears the latter before damage reads
/// them. Without PAINT the new color reconciles into the arena and is never
/// repainted (observed: edited text staying the old color with `damage=0`).
#[test]
#[ntest::timeout(30_000)]
fn reload_patches_text_color_and_requests_repaint() {
    let ink = Rc::new(RefCell::new(Color::WHITE));
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    runtime.borrow_mut().begin_build_window();
    let cx = Cx::new(Rc::clone(&runtime));
    let factory: SharedFactory = {
        let ink = Rc::clone(&ink);
        Rc::new(move || {
            group()
                .width(400.0)
                .height(400.0)
                .children([text("caption")
                    .key("caption")
                    .size(16.0)
                    .color(*ink.borrow())])
                .into_element()
        })
    };
    let root: Element = cx.enter_keyed("root", || factory());
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    runtime.borrow_mut().end_build_window(true);
    engine.set_root_factory(Rc::clone(&factory));
    engine.advance(1.0 / 60.0);
    engine.frame();

    // "Edit the source": the text color changes, then a reload rebuilds.
    *ink.borrow_mut() = Color::BLACK;
    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();

    let inked = engine.with_arena(|arena, _| {
        let key: aurora::foundation::Key = "caption".into();
        arena.node_for_key(&key).map(|id| match &arena.get(id).kind {
            aurora::tree::NodeKind::Text(t) => t.color,
            _ => panic!("caption must stay a text node"),
        })
    });
    assert_eq!(
        inked,
        Some(Color::BLACK),
        "the edited text color lands on the live node"
    );
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_total, 1);
    assert!(
        stats.last_damage_rects >= 1,
        "text repaint was requested, not silently dropped"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn reload_applies_structural_changes_without_resetting_state() {
    let tone = Rc::new(RefCell::new(0x2563EB_u32));
    let extra = Rc::new(RefCell::new(false));
    let (mut engine, _) = mount(Rc::clone(&tone), Rc::clone(&extra));

    press(&mut engine, 50.0, 50.0);
    press(&mut engine, 50.0, 50.0);
    assert!((swatch_height(&engine) - 140.0).abs() < 0.5);

    // "Edit the source": the component gains a child.
    *extra.borrow_mut() = true;
    engine.queue_component_reload("test-component");
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert!(
        (swatch_height(&engine) - 140.0).abs() < 0.5,
        "two taps of hook state survive a structural reload"
    );
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_total, 1);
    assert_eq!(stats.last_reloaded, vec!["test-component".to_string()]);
}

#[test]
#[ntest::timeout(30_000)]
fn panicking_rebuild_keeps_the_previous_tree() {
    let tone = Rc::new(RefCell::new(0x2563EB_u32));
    let extra = Rc::new(RefCell::new(false));
    let (mut engine, _) = mount(Rc::clone(&tone), Rc::clone(&extra));
    let before = swatch_fill(&engine);

    let bad: SharedFactory = Rc::new(|| -> Element { panic!("edited source does not compile") });
    engine.set_root_factory(bad);
    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert_eq!(swatch_fill(&engine), before, "the old tree stays mounted");
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_failed, 1);
    assert!(stats.last_error.is_some(), "the failure is recorded");
    assert!(engine.last_frame_error().is_some(), "the HUD can show it");
    // No frame impact: the rebuild never reached the passes.
    assert!(!stats.last_laid_out);
    assert_eq!(stats.last_nodes_dirtied, 0);
    assert_eq!(stats.last_damage_rects, 0);
}

#[test]
#[ntest::timeout(30_000)]
fn reload_without_a_factory_fails_loudly_and_keeps_the_tree() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(group().width(400.0).height(400.0).key("plain"));
    engine.advance(1.0 / 60.0);
    engine.frame();

    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert!(engine.rect("plain").is_some(), "tree untouched");
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_failed, 1);
    assert!(stats.last_error.is_some());
}

#[test]
#[ntest::timeout(30_000)]
fn set_root_at_patches_one_slot_and_keeps_siblings() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group().width(400.0).height(400.0).children([
            group().key("a").width(100.0).height(100.0).fill(Color::hex(0x111111)),
            group().key("b").width(100.0).height(100.0).fill(Color::hex(0x222222)),
        ]),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();

    let node_a_before = engine.with_arena(|arena, _| {
        let key: aurora::foundation::Key = "a".into();
        arena.node_for_key(&key)
    });

    let parent = engine.with_arena(|_, root| root);
    engine.set_root_at(
        parent,
        1,
        group().key("b").width(100.0).height(100.0).fill(Color::hex(0xDC2626)).into_element(),
    );
    engine.advance(1.0 / 60.0);
    engine.frame();

    let (node_a_after, fill_b) = engine.with_arena(|arena, _| {
        let ka: aurora::foundation::Key = "a".into();
        let kb: aurora::foundation::Key = "b".into();
        (
            arena.node_for_key(&ka),
            arena
                .node_for_key(&kb)
                .and_then(|id| arena.get(id).style.appearance.fill.clone()),
        )
    });
    assert_eq!(node_a_before, node_a_after, "the sibling node is reused");
    assert_eq!(fill_b, Some(Fill::from(Color::hex(0xDC2626))), "the slot is patched");
}

#[test]
#[ntest::timeout(30_000)]
fn component_builds_are_tracked_for_file_scoped_reload() {
    let tone = Rc::new(RefCell::new(0x2563EB_u32));
    let extra = Rc::new(RefCell::new(false));
    let (mut engine, _) = mount(Rc::clone(&tone), Rc::clone(&extra));
    // The drain runs at the start of every frame, so the Swatch build is registered.
    assert!(
        !engine.hot_reload().registry().is_empty(),
        "component builds are tracked"
    );
    // Queueing for an unknown file implicates nothing but still works when forced.
    let hits = engine
        .hot_reload_mut()
        .queue_reload_for_file(std::path::Path::new("src/does_not_exist.rs"));
    assert!(hits.is_empty());
    assert!(!engine.hot_reload().has_pending());
}

#[test]
#[ntest::timeout(30_000)]
fn reloads_do_not_accumulate_frame_registrations() {
    let mut engine = mount_ticking();
    // One interval timer plus one frame clock from the initial build.
    assert_eq!(ticker_count(&engine), 2, "initial registrations");
    for _ in 0..3 {
        engine.queue_full_reload();
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    assert_eq!(
        ticker_count(&engine),
        2,
        "re-running bodies must sweep superseded twins, not stack them"
    );
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_total, 3);
    assert!(engine.has_active_motion(), "the live registrations still drive frames");
}

#[test]
#[ntest::timeout(30_000)]
fn aborted_reload_drops_only_its_own_registrations() {
    let mut engine = mount_ticking();
    assert_eq!(ticker_count(&engine), 2);
    // A factory that registers and then panics: the partial twin must not survive.
    let bad: SharedFactory = Rc::new(|| -> Element {
        use_interval(3600.0, || {});
        panic!("edited source does not compile")
    });
    engine.set_root_factory(bad);
    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        ticker_count(&engine),
        2,
        "the failed build's registrations die with it; the live tree's survive"
    );
    let stats = engine.hot_reload_stats();
    assert_eq!(stats.reloads_failed, 1);
}

#[test]
#[ntest::timeout(30_000)]
fn explicitly_named_component_keeps_state_across_reloads() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    runtime.borrow_mut().begin_build_window();
    let cx = Cx::new(Rc::clone(&runtime));
    let factory: SharedFactory = Rc::new(|| {
        group()
            .width(400.0)
            .height(400.0)
            .children([NamedCard()])
            .into_element()
    });
    let root: Element = cx.enter_keyed("root", || factory());
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    runtime.borrow_mut().end_build_window(true);
    engine.set_root_factory(Rc::clone(&factory));
    engine.advance(1.0 / 60.0);
    engine.frame();

    // The pinned identity (not the module path) is what the registry records.
    assert!(
        engine.hot_reload().registry().contains("pinned::Card"),
        "explicit name is the tracked identity"
    );
    press(&mut engine, 25.0, 25.0);
    let grown = engine.rect("named").map(|r| r.height).unwrap_or(-1.0);
    assert!((grown - 51.0).abs() < 0.5, "tap lands in hook state");

    engine.queue_full_reload();
    engine.advance(1.0 / 60.0);
    engine.frame();
    let kept = engine.rect("named").map(|r| r.height).unwrap_or(-1.0);
    assert!(
        (kept - 51.0).abs() < 0.5,
        "pinned identity recalls across the reload"
    );
}

/// A keyed row holding a frame registration: re-running its body must not stack timers.
#[component]
fn TickingRow(_index: usize, label: String) -> impl IntoElement {
    // A period no test reaches: the timer lives purely to be counted.
    use_interval(3600.0, || {});
    group()
        .key(format!("trow-{label}"))
        .width(Size::fill())
        .height(40.0)
}

/// Reactive rebuilds (reorder, insert) re-run row bodies; superseded registrations must
/// be swept so ticker counts track live rows instead of rebuild history.
///
/// NOTE (residual, pre-existing): removing a row purges its nodes but nothing re-runs
/// its body, so no sweep targets its registrations — a removed row's timer lives until
/// a ticker-epoch retire, exactly as before this machinery existed. Closing that needs
/// a node-to-instance index the reconciler does not keep; deliberately out of scope.
#[test]
#[ntest::timeout(30_000)]
fn reactive_rebuilds_do_not_accumulate_registrations() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = Signal::new(Rc::clone(&runtime), vec!["a".to_string(), "b".to_string()]);
    // One build window spans the factory and the engine creation, mirroring AppConfig:
    // row bodies run during the initial reconcile inside this same generation.
    runtime.borrow_mut().begin_build_window();
    let cx = Cx::new(Rc::clone(&runtime));
    let root: Element = cx.enter_keyed("root", || {
        group()
            .width(400.0)
            .height(400.0)
            .direction(Direction::Vertical)
            .children_labels_keyed(labels.clone(), |s: &String| s.clone(), TickingRow)
            .into_element()
    });
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    runtime.borrow_mut().end_build_window(true);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.advance(1.0 / 60.0);
    engine.frame();
    let initial = ticker_count(&engine);
    assert_eq!(initial, 2, "one timer per mounted row, nothing else");

    // Reorder: every row body re-runs, keyed nodes are reused, twins must die.
    labels.set(vec!["b".to_string(), "a".to_string()]);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        ticker_count(&engine),
        initial,
        "reordered rows must not stack timers"
    );

    // Insert: the new row mounts once; a further reorder changes nothing.
    labels.set(vec!["b".to_string(), "a".to_string(), "c".to_string()]);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.advance(1.0 / 60.0);
    engine.frame();
    let grown = ticker_count(&engine);
    assert_eq!(grown, initial + 1, "exactly the new row adds a timer");
    labels.set(vec!["c".to_string(), "a".to_string(), "b".to_string()]);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        ticker_count(&engine),
        grown,
        "steady after insert: rebuilds replace, never accumulate"
    );
}
