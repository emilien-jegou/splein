// Aurora dev launcher: runs UI crates in hot-reload mode through the shared runtime.
//
// Links nothing aurora-flavored (only clap/libloading/libc), so it can never duplicate
// the runtime. It maps `libaurora_shared.so` globally first — later `dlopen`s (host,
// UI images) resolve aurora symbols to this one instance — then calls the host entry
// point. All configuration crosses as C strings the host reads on entry.
//
// Usage:
//   aurora hot --package aurora-demo1-ui
//   aurora hot --package aurora-hot-ui --title "Hot" --background 0xFFFFFF
//   aurora selftest --ui target/debug/libaurora_hot_ui.so

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
    /// Run the component host against a UI crate, rebuilding live on save.
    ///
    /// Edit the UI crate's sources while the window runs; the loop rebuilds and the
    /// window rebuilds from the fresh library with signals, springs, and tickers
    /// intact. Paths default under `--root` (default: current directory).
    Hot {
        /// Cargo package whose `cdylib` holds the UI (rebuilt on change).
        #[arg(long)]
        package: String,
        /// Workspace root: build working directory and anchor for default paths.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Window title.
        #[arg(long, default_value = "Aurora Hot Reload")]
        title: String,
        /// Window width in pixels.
        #[arg(long, default_value_t = 500)]
        width: u32,
        /// Window height in pixels.
        #[arg(long, default_value_t = 560)]
        height: u32,
        /// Clear color as 0xRRGGBB.
        #[arg(long, default_value = "0xFFFFFF")]
        background: String,
        /// Shared runtime image (default: `<root>/target/debug/libaurora_shared.so`).
        #[arg(long)]
        shared: Option<PathBuf>,
        /// Host library (default: `<root>/target/debug/libhot_host.so`).
        #[arg(long)]
        host: Option<PathBuf>,
    },
    /// Headless check: mount, click, swap entry points, and rebuild a UI image inside
    /// the shared runtime — no window. Exits non-zero with diagnostics on failure.
    Selftest {
        /// UI image to verify.
        #[arg(long)]
        ui: PathBuf,
        /// Shared runtime image (default: `<root>/target/debug/libaurora_shared.so`).
        #[arg(long)]
        shared: Option<PathBuf>,
        /// Host library (default: `<root>/target/debug/libhot_host.so`).
        #[arg(long)]
        host: Option<PathBuf>,
        /// Workspace root anchoring the default paths.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

type HostRunFn = unsafe extern "Rust" fn(
    *const c_char,
    u32,
    u32,
    *const c_char,
    *const c_char,
    u32,
);

/// Headless self-test driver: forwards resolved UI entry points as opaque pointers.
/// The host owns execution (it alone may call into component code); this side only
/// loads and resolves, keeping its dependency set aurora-free.
type HostSelftestFn = unsafe extern "Rust" fn(usize, usize) -> i32;

fn debug_dir(root: &std::path::Path) -> PathBuf {
    root.join("target").join("debug")
}

fn parse_hex(raw: &str) -> u32 {
    let raw = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    u32::from_str_radix(raw, 16).unwrap_or(0xFFFFFF)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Hot {
            package,
            root,
            title,
            width,
            height,
            background,
            shared,
            host,
        } => {
            let debug = debug_dir(&root);
            let shared = shared.unwrap_or_else(|| debug.join("libaurora_shared.so"));
            let host = host.unwrap_or_else(|| debug.join("libhot_host.so"));
            run_host(
                &shared,
                &host,
                &title,
                width,
                height,
                &package,
                &root,
                parse_hex(&background),
            )
        }
        Command::Selftest {
            ui,
            shared,
            host,
            root,
        } => {
            let debug = debug_dir(&root);
            let shared = shared.unwrap_or_else(|| debug.join("libaurora_shared.so"));
            let host = host.unwrap_or_else(|| debug.join("libhot_host.so"));
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
    title: &str,
    width: u32,
    height: u32,
    package: &str,
    root: &std::path::Path,
    background: u32,
) -> ExitCode {
    if preload_shared(shared).is_err() {
        return ExitCode::FAILURE;
    }
    let host_lib = match load_host(host) {
        Ok(lib) => lib,
        Err(code) => return code,
    };
    let run: libloading::Symbol<HostRunFn> = unsafe {
        match host_lib.get(b"hot_host_run") {
            Ok(run) => run,
            Err(e) => {
                eprintln!("aurora: host {} exports no hot_host_run: {e}", host.display());
                return ExitCode::FAILURE;
            }
        }
    };
    let title = CString::new(title).unwrap_or_default();
    let package = CString::new(package).unwrap_or_default();
    let root = CString::new(root.to_string_lossy().as_ref()).unwrap_or_default();
    unsafe {
        run(
            title.as_ptr(),
            width.max(1),
            height.max(1),
            package.as_ptr(),
            root.as_ptr(),
            background,
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
        match host_lib.get(b"hot_host_selftest") {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "aurora: host {} exports no hot_host_selftest: {e}",
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
                    eprintln!("aurora: ui {} exports no aurora_hot_version: {e}", ui.display());
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
