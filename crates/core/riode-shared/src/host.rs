// Hot-reload host loop moved into riode-shared so the shared image also owns devkit entry points.
// (Originally crates/riode-devkit-bin; kept here to remove a second dylib boundary.)

use riode::hot_reload::{
    blocking_hot_build, ComponentReloadExtension, DevLoopConfig, DylibReloader, HotTitleFn,
    HotWindowFn, HotWindowInfo,
};
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use std::ffi::{c_char, CStr};

/// Reads a borrowed C string. The stub keeps every string alive for the call.
fn arg<'a>(ptr: *const c_char) -> &'a str {
    // SAFETY: the stub passes valid NUL-terminated strings that outlive the call.
    unsafe { CStr::from_ptr(ptr).to_str().unwrap_or("") }
}

fn parse_engine_name(raw: &str) -> RenderBackend {
    match raw.to_lowercase().replace('_', "-").as_str() {
        "tiny-skia" | "tinyskia" | "cpu" => RenderBackend::TinySkia,
        "wgpu" | "raw-wgpu" | "rawgpu" => RenderBackend::Wgpu,
        _ => RenderBackend::TinySkia,
    }
}

#[no_mangle]
pub extern "Rust" fn devkit_run(
    package: *const c_char,
    example: *const c_char,
    workspace_root: *const c_char,
    engine: *const c_char,
) {
    riode::hot_reload::init_dev_logging();
    let debugger = riode::app::VisualDebugger::new();
    let example_raw = arg(example);
    let example = (!example_raw.is_empty()).then(|| example_raw.to_string());
    let package_raw = arg(package);
    let root = std::path::PathBuf::from(arg(workspace_root));
    let loop_config = || DevLoopConfig {
        package: package_raw.to_string(),
        example: example.clone(),
        workspace_root: root.clone(),
        watch_dir: example.as_deref().map(|name| root.join("examples").join(name)),
        lib_path: None,
        auto_build: true,
        companion_packages: vec!["riode-shared".to_string(), package_raw.to_string()],
    };
    let build_log = std::env::temp_dir().join(format!("riode-hot-boot-{}.log", std::process::id()));
    if let Err(tail) = blocking_hot_build(&loop_config(), &build_log) {
        eprintln!("riode: hot-reload boot build failed; log: {}", build_log.display());
        for line in &tail {
            eprintln!("riode: hot-reload build | {line}");
        }
        std::process::exit(1);
    }
    let (info, title) = read_boot_chrome(&loop_config());
    let ext = ComponentReloadExtension::new(loop_config());
    let backend = parse_engine_name(arg(engine));
    // Multibackend runner (not single `App`): one shared engine, both
    // factories live, so F7/Tab/1-2 switch rasterizers inside the hot loop.
    // `backend` only picks the focused window at boot.
    riode::app::multibackend::MultiBackend::config([
        Some(crate::tiny_skia::create_backend),
        Some(crate::wgpu::create_backend),
    ])
    .title(format!("{title} [1-2 engine | Tab/F7 cycle]"))
    .size(info.width.max(1), info.height.max(1))
    .font("Inter", include_bytes!("../../riode/assets/inter-var.ttf"))
    .background(Color::hex(info.background))
    .initial(backend)
    .extension(debugger)
    .extension(ext)
    .run(placeholder);
}

fn placeholder() -> impl IntoElement {
    group()
        .width(400.0)
        .height(300.0)
        .children([text("building hot-ui…").size(16.0)])
}

fn read_boot_chrome(config: &DevLoopConfig) -> (HotWindowInfo, String) {
    let fallback = (HotWindowInfo::fallback(), "Aurora Hot Reload".to_string());
    let mut loader = DylibReloader::new(config.lib_path());
    if !loader.refresh() {
        eprintln!("riode: hot-reload boot image not loadable; default chrome");
        return fallback;
    }
    let skewed = unsafe {
        match loader.get::<riode::hot_reload::ComponentVersionFn>(b"riode_hot_version") {
            Ok(f) => f() != riode::hot_reload::component_abi_fingerprint(),
            Err(_) => true,
        }
    };
    if skewed {
        eprintln!("riode: hot-reload boot image handshake failed; default chrome");
        return fallback;
    }
    let info = unsafe {
        match loader.get::<HotWindowFn>(b"riode_hot_window") {
            Ok(f) => f(),
            Err(_) => HotWindowInfo::fallback(),
        }
    };
    let title = unsafe {
        match loader.get::<HotTitleFn>(b"riode_hot_title") {
            Ok(f) => f().to_string(),
            Err(_) => fallback.1.clone(),
        }
    };
    (info, title)
}

#[no_mangle]
pub extern "Rust" fn devkit_selftest(version_fn: usize, root_fn: usize) -> i32 {
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
    use riode::hot_reload::{
        component_abi_fingerprint, ComponentRootFn, ComponentVersionFn, SharedFactory,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let version_fn: ComponentVersionFn = unsafe { std::mem::transmute(version_fn) };
    let root_fn: ComponentRootFn = unsafe { std::mem::transmute(root_fn) };
    let run = || -> Result<(), i32> {
        check(
            version_fn() == component_abi_fingerprint(),
            "protocol handshake agrees",
        )?;

        let runtime = Rc::new(RefCell::new(riode::reactive::ReactiveRuntime::new()));
        runtime.borrow_mut().begin_build_window();
        let cx = riode::reactive::Cx::new(Rc::clone(&runtime));
        let current = Rc::new(Cell::new(Some(root_fn)));
        let factory: SharedFactory = {
            let current = Rc::clone(&current);
            Rc::new(move || {
                let build = current.get().expect("component entry installed");
                build()
            })
        };
        let tree = cx.enter_keyed("root", || factory());
        let mut engine = riode::runtime::Engine::with_runtime(Rc::clone(&runtime), tree, 500, 560);
        runtime.borrow_mut().end_build_window(true);
        engine.set_root_factory(Rc::clone(&factory));
        engine.advance(1.0 / 60.0);
        engine.frame();

        check(
            engine.hot_reload().registry().contains("hot_ui::Counter"),
            "dylib component scopes tracked",
        )?;
        let hero_fill = engine.with_arena(|arena, _| {
            let key: riode::foundation::Key = "hero".into();
            arena
                .node_for_key(&key)
                .and_then(|id| arena.get(id).style.appearance.fill.clone())
        });
        check(
            hero_fill
                == Some(riode::foundation::Fill::from(
                    riode::foundation::Color::hex(0xDC2626),
                )),
            "dylib hero color lands on the live node",
        )?;
        let height = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check(
            (height - 100.0).abs() < 0.5,
            "counter mounts at base height",
        )?;

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
        check(
            (tapped - 120.0).abs() < 0.5,
            "tap lands in dylib hook state",
        )?;

        current.set(Some(root_fn));
        engine.queue_full_reload();
        engine.advance(1.0 / 60.0);
        engine.frame();
        let kept = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check(
            (kept - 120.0).abs() < 0.5,
            "taps survive rebuild from swapped code",
        )?;
        let stats = engine.hot_reload_stats();
        check(stats.reloads_total == 1, "exactly one reload recorded")?;
        check(stats.last_error.is_none(), "no reload error recorded")?;

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
