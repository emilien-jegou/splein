// Hot-reload host loop: window, engine, and component code-swap driver.
//
// Built as a `dylib` linking only `aurora-shared`, so it shares the process's one aurora
// instance with every UI cdylib. Exports a single C-compatible entry point the thin
// `hot-stub` launcher calls; everything else (event loop, backends, reload extension) is
// ordinary aurora API resolving through the shared image.
//
// All arguments cross as primitives and borrowed C strings (read during the call), so no
// shared struct layout is required between stub and host beyond this signature.

extern crate aurora_shared as aurora;

use aurora::hot_reload::{ComponentReloadExtension, DevLoopConfig};
use aurora::prelude::*;
use std::ffi::{c_char, CStr};

/// Reads a borrowed C string. The stub keeps every string alive for the call.
fn arg<'a>(ptr: *const c_char) -> &'a str {
    // SAFETY: the stub passes valid NUL-terminated strings that outlive the call.
    unsafe { CStr::from_ptr(ptr).to_str().unwrap_or("") }
}

/// Runs the hot-reload host: opens the window, loads the UI crate, and drives the
/// watch → rebuild → swap loop until exit. Never returns until the window closes.
///
/// `background` is a `0xRRGGBB` clear color. `package` names the UI crate rebuilt on
/// change; `workspace_root` anchors the build and default paths.
#[no_mangle]
pub extern "Rust" fn hot_host_run(
    title: *const c_char,
    width: u32,
    height: u32,
    package: *const c_char,
    workspace_root: *const c_char,
    background: u32,
) {
    aurora::hot_reload::init_dev_logging();
    let debugger = aurora::app::VisualDebugger::new();
    let ext = ComponentReloadExtension::new(DevLoopConfig {
        package: arg(package).to_string(),
        workspace_root: arg(workspace_root).into(),
        watch_dir: None,
        lib_path: None,
        auto_build: true,
    });
    aurora::app::App::config()
        .title(arg(title))
        .size(width.max(1), height.max(1))
        .background(Color::hex(background))
        .backend(RenderBackend::TinySkia)
        .backend_factory(aurora_shared::tiny_skia::create_backend)
        .extension(debugger)
        .extension(ext)
        .run(placeholder);
}

/// Shown until the first UI library finishes loading.
fn placeholder() -> impl IntoElement {
    group().width(400.0).height(300.0).children([text(
        "building hot-ui…",
    )
    .size(16.0)])
}

/// Headless self-test: mount, click, swap entry points, rebuild, verify — entirely inside
/// the shared instance (no window, no event loop).
///
/// The stub resolves the UI's entry points and forwards them as raw pointers (it owns the
/// dlopen; this side must not touch libloading, keeping its dependency set to exactly
/// one crate). Returns 0 when every check passes, 1 otherwise, with diagnostics on
/// stderr. Never unwinds across the boundary: the whole body runs under `catch_unwind`.
///
/// Checks: component scopes tracked, counter mounts, tap lands in dylib hook state,
/// re-resolved entry points rebuild with taps preserved and a clean report.
#[no_mangle]
pub extern "Rust" fn hot_host_selftest(version_fn: usize, root_fn: usize) -> i32 {
    let outcome = std::panic::catch_unwind(|| selftest_body(version_fn, root_fn));
    match outcome {
        Ok(0) => 0,
        Ok(code) => code,
        Err(_) => {
            eprintln!("hot selftest: panicked; see logs above");
            1
        }
    }
}

fn check(cond: bool, what: &str) -> Result<(), i32> {
    if cond {
        Ok(())
    } else {
        eprintln!("hot selftest FAILED: {what}");
        Err(1)
    }
}

fn selftest_body(version_fn: usize, root_fn: usize) -> i32 {
    use aurora::hot_reload::{
        ComponentRootFn, ComponentVersionFn, SharedFactory, COMPONENT_ABI_VERSION,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    // SAFETY: the stub forwards pointers resolved from the loaded UI image, same
    // toolchain both sides (the loop's documented contract).
    let version_fn: ComponentVersionFn = unsafe { std::mem::transmute(version_fn) };
    let root_fn: ComponentRootFn = unsafe { std::mem::transmute(root_fn) };
    let run = || -> Result<(), i32> {
        check(version_fn() == COMPONENT_ABI_VERSION, "protocol handshake agrees")?;

        let runtime = Rc::new(RefCell::new(aurora::reactive::ReactiveRuntime::new()));
        runtime.borrow_mut().begin_build_window();
        let cx = aurora::reactive::Cx::new(Rc::clone(&runtime));
        let current = Rc::new(Cell::new(Some(root_fn)));
        let factory: SharedFactory = {
            let current = Rc::clone(&current);
            Rc::new(move || {
                let build = current.get().expect("component entry installed");
                build()
            })
        };
        let tree = cx.enter_keyed("root", || factory());
        let mut engine = aurora::runtime::Engine::with_runtime(Rc::clone(&runtime), tree, 500, 560);
        runtime.borrow_mut().end_build_window(true);
        engine.set_root_factory(Rc::clone(&factory));
        engine.advance(1.0 / 60.0);
        engine.frame();

        check(
            engine.hot_reload().registry().contains("hot_ui::Counter"),
            "dylib component scopes tracked",
        )?;
        // NOTE: tracks the demo source (`hero` fill in aurora-hot-ui); update together.
        let hero_fill = engine.with_arena(|arena, _| {
            let key: aurora::foundation::Key = "hero".into();
            arena
                .node_for_key(&key)
                .and_then(|id| arena.get(id).style.appearance.fill.clone())
        });
        check(
            hero_fill
                == Some(aurora::foundation::Fill::from(
                    aurora::foundation::Color::hex(0xDC2626),
                )),
            "dylib hero color lands on the live node",
        )?;
        let height = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check((height - 100.0).abs() < 0.5, "counter mounts at base height")?;

        // Click the counter card.
        let (x, y) = engine
            .rect("counter")
            .map(|r| (r.x + r.width / 2.0, r.y + r.height / 2.0))
            .expect("counter mounted");
        engine.queue_pointer_down(x, y);
        engine.advance(1.0 / 60.0);
        engine.frame();
        engine.queue_pointer_up(x, y);
        engine.advance(1.0 / 60.0);
        engine.frame();
        let tapped = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check((tapped - 120.0).abs() < 0.5, "tap lands in dylib hook state")?;

        // Swap entry points (stands in for a rebuilt library) and rebuild publicly.
        current.set(Some(root_fn));
        engine.queue_full_reload();
        engine.advance(1.0 / 60.0);
        engine.frame();
        let kept = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check((kept - 120.0).abs() < 0.5, "taps survive rebuild from swapped code")?;
        let stats = engine.hot_reload_stats();
        check(stats.reloads_total == 1, "exactly one reload recorded")?;
        check(stats.last_error.is_none(), "no reload error recorded")?;

        // A dylib panic must unwind catchably across the boundary, never abort the
        // process: calling the entry outside any hook scope panics inside dylib code
        // ("hook called outside a component body"). If personalities disagree, this
        // kills the process instead of returning Err — that outcome fails loudly here
        // rather than in production. The payload must also survive as data.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| root_fn()));
        check(outcome.is_err(), "unscoped entry call panics")?;
        let msg = match outcome {
            Err(payload) => payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_default(),
            Ok(_) => String::new(),
        };
        check(
            msg.contains("component body"),
            "panic payload crosses the boundary intact",
        )?;
        Ok(())
    };
    match run() {
        Ok(()) => {
            eprintln!("hot selftest: all checks passed");
            0
        }
        Err(code) => code,
    }
}
