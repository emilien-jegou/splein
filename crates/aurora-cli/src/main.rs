// Aurora dev launcher: runs UI crates in hot-reload mode through the shared runtime.
//
// Links nothing aurora-flavored (only clap/libloading/libc), so it can never duplicate
// the runtime. It maps `libaurora_shared.so` globally first — later `dlopen`s (host,
// UI images) resolve aurora symbols to this one instance — then calls the host entry
// point. All configuration crosses as C strings the host reads on entry.
//
// The window boots hidden at neutral chrome; the host's reload extension dresses
// it (title/size/background from the UI image's `aurora_hot_window` /
// `aurora_hot_title` symbols) once the first UI library installs.
// No geometry is passed anywhere pre-build.
//
// Usage:
//   aurora hot --example motion_press [--engine vello]
//   aurora selftest --ui target/release/libaurora_hot_ui.so

use std::ffi::{c_char, CString};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Aurora dev launcher: hot-reload hosts and headless UI checks.
#[derive(Parser)]
#[command(name = "aurora", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the component host against an example, rebuilding live on save.
    ///
    /// Edit the example's sources while the window runs; the loop rebuilds and the
    /// window rebuilds from the fresh library with signals, springs, and tickers
    /// intact. The window boots hidden and is dressed (title/size/background
    /// from the UI image's window symbols) on first load. Paths default
    /// under `--root` (default: current directory).
    Hot {
        /// Cargo package whose `cdylib` holds the UI (rebuilt on change).
        /// Legacy per-example packages; prefer `--example` (generative UI).
        #[arg(long, default_value = "aurora-hot-gen-ui")]
        package: String,
        /// Generative-UI example name (`examples/<name>/`): the shared UI
        /// crate builds that example's `view.rs`/`hot.rs`. Replaces the
        /// per-example `-ui` packages entirely.
        #[arg(long)]
        example: Option<String>,
        /// Workspace root: build working directory and anchor for default paths.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Shared runtime image (default: next to this launcher binary).
        #[arg(long)]
        shared: Option<PathBuf>,
        /// Host library (default: next to this launcher binary).
        #[arg(long)]
        host: Option<PathBuf>,
        /// Raster engine: tiny-skia (default), vello, vello-cpu, wgpu.
        #[arg(long)]
        engine: Option<String>,
    },
    /// Headless check: mount, click, swap entry points, and rebuild a UI image inside
    /// the shared runtime — no window. Exits non-zero with diagnostics on failure.
    Selftest {
        /// UI image to verify.
        #[arg(long)]
        ui: PathBuf,
        /// Shared runtime image (default: next to this launcher binary).
        #[arg(long)]
        shared: Option<PathBuf>,
        /// Host library (default: next to this launcher binary).
        #[arg(long)]
        host: Option<PathBuf>,
        /// Workspace root anchoring the default paths.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

type HostRunFn =
    unsafe extern "Rust" fn(*const c_char, *const c_char, *const c_char, *const c_char);

/// Headless self-test driver: forwards resolved UI entry points as opaque pointers.
/// The host owns execution (it alone may call into component code); this side only
/// loads and resolves, keeping its dependency set aurora-free.
type HostSelftestFn = unsafe extern "Rust" fn(usize, usize) -> i32;

/// Directory holding the shared/host images: the directory containing this
/// launcher binary. `example.sh` builds launcher and images with one profile
/// into one directory, so the launcher always sits next to what it loads —
/// no target/profile folder strings anywhere. Explicit `--shared`/`--host`
/// flags still win.
fn image_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

/// Resolves one image path: explicit flag first, then the launcher's sibling
/// directory (which must exist — otherwise nothing was built). Fails loud
/// instead of dlopening a stale image from elsewhere.
fn resolve_image(explicit: Option<PathBuf>, file: &str) -> Result<PathBuf, ExitCode> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = image_dir().map(|dir| dir.join(file)) {
        if path.exists() {
            return Ok(path);
        }
    }
    eprintln!("aurora: {file} not found next to the launcher; build first (./example.sh <name>) or pass its path explicitly");
    Err(ExitCode::FAILURE)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Hot {
            package,
            example,
            root,
            shared,
            host,
            engine,
        } => {
            let shared = match resolve_image(shared, "libaurora_shared.so") {
                Ok(path) => path,
                Err(code) => return code,
            };
            let host = match host {
                Some(path) => match resolve_image(Some(path), "libaurora_devkit_bin.so") {
                    Ok(path) => path,
                    Err(code) => return code,
                },
                None => shared.clone(),
            };
            run_host(
                &shared,
                &host,
                &package,
                example.as_deref().unwrap_or(""),
                &root,
                engine.as_deref(),
            )
        }
        Command::Selftest {
            ui,
            shared,
            host,
            root: _root,
        } => {
            let shared = match resolve_image(shared, "libaurora_shared.so") {
                Ok(path) => path,
                Err(code) => return code,
            };
            let host = match host {
                Some(path) => match resolve_image(Some(path), "libaurora_devkit_bin.so") {
                    Ok(path) => path,
                    Err(code) => return code,
                },
                None => shared.clone(),
            };
            run_selftest(&shared, &host, &ui)
        }
    }
}

/// Maps the shared runtime globally FIRST, then runs the host entry point.
///
/// Leaked, never closed: dlclose ordering against thread-locals and per-image
/// destructors segfaults on exit, and a dev-loop launcher has no reason to unmap what
/// the OS reclaims.
fn preload_shared(shared: &std::path::Path) -> Result<(), ExitCode> {
    let path = shared.to_string_lossy();
    unsafe {
        match libloading::os::unix::Library::open(
            Some(path.as_ref()),
            libc::RTLD_NOW | libc::RTLD_GLOBAL,
        ) {
            Ok(lib) => {
                std::mem::forget(lib);
                Ok(())
            }
            Err(e) => {
                eprintln!("aurora: cannot preload shared runtime {path}: {e}");
                Err(ExitCode::FAILURE)
            }
        }
    }
}

fn load_host(host: &std::path::Path) -> Result<libloading::Library, ExitCode> {
    let path = host.to_string_lossy();
    unsafe {
        match libloading::Library::new(path.as_ref()) {
            Ok(lib) => Ok(lib),
            Err(e) => {
                eprintln!("aurora: cannot load host {path}: {e}");
                Err(ExitCode::FAILURE)
            }
        }
    }
}

fn run_host(
    shared: &std::path::Path,
    host: &std::path::Path,
    package: &str,
    example: &str,
    root: &std::path::Path,
    engine: Option<&str>,
) -> ExitCode {
    if preload_shared(shared).is_err() {
        return ExitCode::FAILURE;
    }
    let host_lib = match load_host(host) {
        Ok(lib) => lib,
        Err(code) => return code,
    };
    let run: libloading::Symbol<HostRunFn> = unsafe {
        match host_lib.get(b"devkit_run") {
            Ok(run) => run,
            Err(e) => {
                eprintln!("aurora: host {} exports no devkit_run: {e}", host.display());
                return ExitCode::FAILURE;
            }
        }
    };
    let package = CString::new(package).unwrap_or_default();
    let example = CString::new(example).unwrap_or_default();
    let root = CString::new(root.to_string_lossy().as_ref()).unwrap_or_default();
    let engine = CString::new(engine.unwrap_or("")).unwrap_or_default();
    unsafe {
        run(
            package.as_ptr(),
            example.as_ptr(),
            root.as_ptr(),
            engine.as_ptr(),
        )
    };
    // The event loop only returns on exit: leak the image (see above) so teardown
    // cannot trip over dlclose ordering either.
    std::mem::forget(host_lib);
    ExitCode::SUCCESS
}

fn run_selftest(
    shared: &std::path::Path,
    host: &std::path::Path,
    ui: &std::path::Path,
) -> ExitCode {
    if preload_shared(shared).is_err() {
        return ExitCode::FAILURE;
    }
    // Leak the host too on this path: no dlclose ordering hazards on the way out.
    let host_lib = match load_host(host) {
        Ok(lib) => lib,
        Err(code) => return code,
    };
    let selftest: libloading::Symbol<HostSelftestFn> = unsafe {
        match host_lib.get(b"devkit_selftest") {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "aurora: host {} exports no devkit_selftest: {e}",
                    host.display()
                );
                return ExitCode::FAILURE;
            }
        }
    };
    let ui_lib = unsafe {
        match libloading::Library::new(ui) {
            Ok(lib) => lib,
            Err(e) => {
                eprintln!("aurora: cannot load ui {}: {e}", ui.display());
                return ExitCode::FAILURE;
            }
        }
    };
    // Opaque addresses only: the launcher never calls UI code (doing so through a
    // mismatched signature would be UB). The host transmutes them back to the real
    // signatures (same toolchain both sides) and executes them in-image.
    let (version_ptr, root_ptr): (usize, usize) = unsafe {
        let version: libloading::Symbol<*mut std::ffi::c_void> =
            match ui_lib.get(b"aurora_hot_version") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "aurora: ui {} exports no aurora_hot_version: {e}",
                        ui.display()
                    );
                    return ExitCode::FAILURE;
                }
            };
        let root: libloading::Symbol<*mut std::ffi::c_void> =
            match ui_lib.get(b"aurora_component_root") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "aurora: ui {} exports no aurora_component_root: {e}",
                        ui.display()
                    );
                    return ExitCode::FAILURE;
                }
            };
        (*version as usize, *root as usize)
    };
    let code = unsafe { selftest(version_ptr, root_ptr) };
    std::mem::forget(ui_lib);
    std::mem::forget(host_lib);
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(code as u8)
    }
}
