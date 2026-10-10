// Single responsibility: End-to-end proof that a real UI cdylib drives a headless engine.
//
// Loads the actual `libaurora_hot_ui.so` through `DylibReloader` (the same path the
// `hot_demo` host uses), interprets its description into a live tree, clicks through to
// state writes, and rebuilds from re-resolved symbols with state intact. This pins the
// dylib boundary — data across, signals host-side — without needing a window.
//
// NOTE: the hero-fill assertion tracks `crates/aurora-hot-ui/src/lib.rs`; if the demo's
// color changes there, update the expected value here.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use aurora::hot_reload::remote::{
    RemoteBuildFn, RemoteEventFn, RemoteHost, RemoteStateFn, RemoteVersionFn,
    remote_abi_fingerprint,
};
use aurora::hot_reload::DylibReloader;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use aurora::runtime::Engine;

/// Path of the demo UI image, built on demand when missing.
fn lib_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("debug")
        .join("libaurora_hot_ui.so")
}

fn ensure_built() {
    if lib_path().exists() {
        return;
    }
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("aurora-hot-ui")
        .status()
        .expect("cargo must run to build the hot-ui cdylib");
    assert!(status.success(), "cargo build -p aurora-hot-ui must succeed");
    assert!(lib_path().exists(), "the cdylib image must exist after building");
}

fn press(engine: &mut Engine, x: f32, y: f32) {
    engine.queue_pointer_down(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.queue_pointer_up(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
}

fn hero_fill(engine: &Engine) -> Option<aurora::foundation::Fill> {
    engine.with_arena(|arena, _| {
        let key: aurora::foundation::Key = "hero".into();
        arena
            .node_for_key(&key)
            .and_then(|id| arena.get(id).style.appearance.fill.clone())
    })
}

#[test]
#[ntest::timeout(120_000)]
fn real_dylib_drives_interaction_and_survives_rebuild() {
    ensure_built();
    let mut loader = DylibReloader::new(lib_path());
    assert!(loader.refresh(), "the demo library must load");
    assert!(loader.is_loaded());

    // Copy the entry points out of their symbols: raw fn pointers, no lifetimes.
    // The version handshake mirrors the extension: drift fails closed, never silently.
    let version_fn: RemoteVersionFn = unsafe {
        *loader
            .get(b"aurora_remote_version")
            .expect("aurora_remote_version")
    };
    assert_eq!(
        version_fn(),
        remote_abi_fingerprint(),
        "protocol handshake across the boundary"
    );
    let (state_fn, build_fn, event_fn): (RemoteStateFn, RemoteBuildFn, RemoteEventFn) = unsafe {
        (
            *loader
                .get(b"aurora_remote_state")
                .expect("aurora_remote_state"),
            *loader.get(b"aurora_remote_build").expect("aurora_remote_build"),
            *loader.get(b"aurora_remote_event").expect("aurora_remote_event"),
        )
    };

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let host = RemoteHost::new();
    host.attach_runtime(Rc::clone(&runtime));
    host.set_fns(state_fn, build_fn, event_fn);
    assert!(host.ensure_state(&state_fn()));
    assert!(host.signal_count() >= 2, "declares taps + open");

    let node = build_fn(&host.snapshot());
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), host.interpret(&node), 500, 560);
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert_eq!(
        hero_fill(&engine),
        Some(Fill::from(Color::hex(0xDC2626))),
        "the dylib's current hero color lands on the live node"
    );

    // Click the counter card: the interpreted on_click queues "tap".
    let center = engine
        .rect("card")
        .map(|r| (r.x + r.width / 2.0, r.y + r.height / 2.0))
        .expect("card mounted");
    press(&mut engine, center.0, center.1);
    let actions = host.drain_actions();
    assert_eq!(actions, vec!["tap".to_string()]);
    let mut writes = Vec::new();
    for action in &actions {
        writes.extend(event_fn(action));
    }
    host.apply_writes(&writes);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        host.snapshot().get_f32("taps"),
        1.0,
        "click → action → write → live signal"
    );

    // Re-resolve symbols (stands in for a rebuilt library) and rebuild the tree the way
    // the engine drain does: new description, same host signals.
    let (state_fn2, build_fn2, event_fn2): (RemoteStateFn, RemoteBuildFn, RemoteEventFn) =
        unsafe {
            (
                *loader.get(b"aurora_remote_state").expect("state"),
                *loader.get(b"aurora_remote_build").expect("build"),
                *loader.get(b"aurora_remote_event").expect("event"),
            )
        };
    host.set_fns(state_fn2, build_fn2, event_fn2);
    assert!(host.ensure_state(&state_fn2()));
    let node2 = build_fn2(&host.snapshot());
    engine.set_root(host.interpret(&node2));
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert_eq!(
        host.snapshot().get_f32("taps"),
        1.0,
        "tap count survives a full rebuild from re-resolved code"
    );
    assert!(engine.rect("card").is_some(), "tree intact after rebuild");
}
