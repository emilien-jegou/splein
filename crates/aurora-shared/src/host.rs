// Hot-reload host loop moved into aurora-shared so the shared image also owns devkit entry points.
// (Originally crates/aurora-devkit-bin; kept here to remove a second dylib boundary.)

use aurora::hot_reload::{
    blocking_hot_build, ComponentReloadExtension, DevLoopConfig, DylibReloader, HotTitleFn,
    HotWindowFn, HotWindowInfo,
};
use aurora::prelude::*;
use std::ffi::{c_char, CStr};

/// Reads a borrowed C string. The stub keeps every string alive for the call.
fn arg<'a>(ptr: *const c_char) -> &'a str {
    // SAFETY: the stub passes valid NUL-terminated strings that outlive the call.
    unsafe { CStr::from_ptr(ptr).to_str().unwrap_or("") }
}

fn parse_engine_name(raw: &str) -> RenderBackend {
    match raw.to_lowercase().replace('_', "-").as_str() {
        "tiny-skia" | "tinyskia" | "skia" | "cpu" => RenderBackend::TinySkia,
        "vello" | "vello-gpu" | "gpu" => RenderBackend::Vello,
        "vello-cpu" | "cpu-vello" | "vellocpu" => RenderBackend::VelloCpu,
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
    aurora::hot_reload::init_dev_logging();
    let debugger = aurora::app::VisualDebugger::new();
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
        companion_packages: vec!["aurora-shared".to_string(), package_raw.to_string()],
    };
    let build_log = std::env::temp_dir().join(format!("aurora-hot-boot-{}.log", std::process::id()));
    if let Err(tail) = blocking_hot_build(&loop_config(), &build_log) {
        eprintln!("aurora: hot-reload boot build failed; log: {}", build_log.display());
        for line in &tail {
            eprintln!("aurora: hot-reload build | {line}");
        }
        std::process::exit(1);
    }
    let (info, title) = read_boot_chrome(&loop_config());
    let ext = ComponentReloadExtension::new(loop_config());
    let backend = parse_engine_name(arg(engine));
    let mut config = aurora::app::App::config()
        .title(title)
        .size(info.width.max(1), info.height.max(1))
        .font("Inter", include_bytes!("../../aurora/assets/inter-var.ttf"))
        .background(Color::hex(info.background))
        .backend(backend);
    config = match backend {
        RenderBackend::TinySkia => config.backend_factory(crate::tiny_skia::create_backend),
        RenderBackend::Vello => config.backend_factory(crate::vello::create_backend),
        RenderBackend::VelloCpu => config.backend_factory(crate::vello::create_cpu_backend),
        RenderBackend::Wgpu => config.backend_factory(crate::wgpu::create_backend),
    };
    config
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
        eprintln!("aurora: hot-reload boot image not loadable; default chrome");
        return fallback;
    }
    let skewed = unsafe {
        match loader.get::<aurora::hot_reload::ComponentVersionFn>(b"aurora_hot_version") {
            Ok(f) => f() != aurora::hot_reload::component_abi_fingerprint(),
            Err(_) => true,
        }
    };
    if skewed {
        eprintln!("aurora: hot-reload boot image handshake failed; default chrome");
        return fallback;
    }
    let info = unsafe {
        match loader.get::<HotWindowFn>(b"aurora_hot_window") {
            Ok(f) => f(),
            Err(_) => HotWindowInfo::fallback(),
        }
    };
    let title = unsafe {
        match loader.get::<HotTitleFn>(b"aurora_hot_title") {
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
    use aurora::hot_reload::{
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
