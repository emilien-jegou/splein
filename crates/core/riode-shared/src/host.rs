// Responsibility: Host runner entry point (`devkit_run`) for running UI crates under the shared runtime.

use riode::hot_reload::builder::cargo::CargoRunner;
use riode::hot_reload::builder::BuildRunner;
use riode::hot_reload::contract::LoadedUiContract;
use riode::hot_reload::{ComponentReloadExtension, DylibReloader, HotWindowInfo};
use riode::internal::group;
use riode::prelude::*;
use std::ffi::{c_char, CStr};
use std::path::PathBuf;

fn parse_c_str<'a>(ptr: *const c_char) -> &'a str {
    unsafe { CStr::from_ptr(ptr).to_str().unwrap_or("") }
}

fn read_boot_chrome(runner: &dyn BuildRunner) -> (HotWindowInfo, String) {
    let mut loader = DylibReloader::new(runner.lib_path());
    if !loader.refresh() {
        return (HotWindowInfo::fallback(), "Aurora Hot Reload".to_string());
    }
    match LoadedUiContract::resolve(&loader) {
        Ok(contract) => (contract.window_info, contract.window_title),
        Err(err) => {
            eprintln!("riode: boot image contract invalid ({err}); using fallback chrome");
            (HotWindowInfo::fallback(), "Aurora Hot Reload".to_string())
        }
    }
}

fn placeholder() -> impl IntoElement {
    group()
        .width(400.0)
        .height(300.0)
        .children([riode::internal::text("building hot-ui…").size(16.0)])
}

#[no_mangle]
pub extern "Rust" fn devkit_run(
    package: *const c_char,
    example: *const c_char,
    workspace_root: *const c_char,
) {
    riode::hot_reload::init_dev_logging();
    let example_raw = parse_c_str(example);
    let example = (!example_raw.is_empty()).then(|| example_raw.to_string());
    let package = parse_c_str(package).to_string();
    let root = PathBuf::from(parse_c_str(workspace_root));

    let watch_dir = example.as_deref().map(|name| {
        let ex_path = root.join("examples").join(name);
        if ex_path.exists() {
            ex_path
        } else {
            root.join("src")
        }
    });

    let runner = CargoRunner::builder()
        .package(package)
        .maybe_example(example)
        .workspace_root(root)
        .maybe_watch_dir(watch_dir)
        .build();

    let build_log = std::env::temp_dir().join(format!("riode-hot-boot-{}.log", std::process::id()));
    if let Err(tail) = runner.blocking_build(&build_log) {
        eprintln!("riode: hot-reload boot build failed; log: {}", build_log.display());
        for line in &tail {
            eprintln!("riode: hot-reload build | {line}");
        }
        std::process::exit(1);
    }

    let (info, title) = read_boot_chrome(&runner);
    let ext = ComponentReloadExtension::new(Box::new(runner));

    App::config()
        .title(title)
        .size(info.width.max(1), info.height.max(1))
        .font("Inter", include_bytes!("../../riode/assets/inter-var.ttf"))
        .background(Color::hex(info.background))
        .backend_factory(crate::wgpu::create_backend)
        .extension(ext)
        .run(placeholder);
}
