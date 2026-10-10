// Single responsibility: The dev-loop cycle works against real images, without
// executing component code in this process.
//
// Rationale (read before "fixing" this test to do more): a cargo test binary links
// aurora statically, so it holds a different runtime instance than any dlopened image.
// Calling a component entry point here would resolve hooks against the wrong instance
// (`hook called outside a component body`, then SIGABRT on unwind). Component execution
// is therefore verified inside shared-instance land only — see `hot_host_selftest`,
// driven headlessly by `hot-stub --selftest`. What this test pins instead:
// - the DevLoop reaches `LibraryReady` (watch → build → refresh, real cargo processes),
// - both images load in dependency order with symbols resolving,
// - the version handshake agrees on both sides.
//
// A source touch mid-test forces a genuine second cycle (not a re-resolve of the same
// bytes): retired-image count must advance.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use aurora::hot_reload::{ComponentVersionFn, DevLoop, DevLoopConfig, PollOutcome, COMPONENT_ABI_VERSION};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn debug_dir() -> PathBuf {
    workspace_root().join("target").join("debug")
}

/// Polls until `LibraryReady`. Panics past `deadline`.
fn await_library(dev: &mut DevLoop, deadline: Instant, what: &str) {
    loop {
        match dev.poll() {
            PollOutcome::LibraryReady => return,
            PollOutcome::Idle | PollOutcome::Building | PollOutcome::Settled => {}
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what} from the dev loop"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn version_of(ui_path: &std::path::Path) -> u32 {
    let ui = unsafe { libloading::Library::new(ui_path).expect("ui image loads") };
    let version: libloading::Symbol<ComponentVersionFn> =
        unsafe { ui.get(b"aurora_hot_version").expect("version symbol") };
    version()
}

#[test]
#[ntest::timeout(600_000)]
fn dev_loop_cycles_and_handshake_agrees() {
    let root = workspace_root();
    let mut dev = DevLoop::new(DevLoopConfig {
        package: "aurora-hot-ui".to_string(),
        workspace_root: root.clone(),
        watch_dir: None,
        lib_path: None,
        auto_build: true,
    });
    await_library(&mut dev, Instant::now() + Duration::from_secs(420), "initial library");

    // The shared image must exist (the loop built or found it); the UI image too.
    let shared_path = debug_dir().join("libaurora_shared.so");
    let ui_path = debug_dir().join("libaurora_hot_ui.so");
    assert!(shared_path.exists(), "shared image present");
    assert!(ui_path.exists(), "ui image present");
    assert_eq!(
        version_of(&ui_path),
        COMPONENT_ABI_VERSION,
        "protocol handshake across the boundary"
    );

    // Touch the sources (content unchanged) and ride a genuine second cycle: the rebuild
    // must produce a superseding image, not just re-resolve the same bytes.
    let ui_src = root
        .join("crates")
        .join("aurora-hot-ui")
        .join("src")
        .join("lib.rs");
    let bytes = std::fs::read(&ui_src).expect("ui sources readable");
    std::fs::write(&ui_src, &bytes).expect("touch ui sources");
    let retired_before = dev.loader().retired_count();
    await_library(&mut dev, Instant::now() + Duration::from_secs(420), "rebuilt library");
    assert!(
        dev.loader().retired_count() > retired_before,
        "the rebuilt image superseded the live one"
    );
    assert_eq!(
        version_of(&ui_path),
        COMPONENT_ABI_VERSION,
        "handshake still agrees after the swap"
    );
}
