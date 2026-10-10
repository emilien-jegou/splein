// Thin launcher for hot-reload hosts: preloads the shared runtime, then runs the host.
//
// The stub links nothing aurora-flavored (only `libloading`/`libc` statically), so it can
// never duplicate the runtime. It maps `libaurora_shared.so` globally first — later
// `dlopen`s (host, UI images) resolve aurora symbols to this one instance — then calls
// the host entry point. All configuration crosses as C strings the host reads on entry.
//
// Usage: `hot-stub --shared <libaurora_shared.so> --host <libhot_host.so>
//   --package aurora-hot-ui --root /path/to/workspace
//   [--title T] [--width W] [--height H] [--background 0xRRGGBB]`

use std::ffi::{c_char, CString};
use std::process::ExitCode;

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

fn usage() -> ! {
    eprintln!(
        "usage: hot-stub --shared <libaurora_shared.so> --host <libhot_host.so> \
         --package <ui-crate> --root <workspace> [--title T] [--width W] [--height H] \
         [--background 0xRRGGBB] | hot-stub --shared <so> --host <so> --selftest <ui.so>"
    );
    std::process::exit(2);
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn parse_hex(raw: &str) -> u32 {
    let raw = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")).unwrap_or(raw);
    u32::from_str_radix(raw, 16).unwrap_or(0xFFFFFF)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(shared) = flag_value(&args, "--shared") else {
        usage()
    };
    let Some(host) = flag_value(&args, "--host") else {
        usage()
    };
    // Self-test needs no package/root/title: it drives a headless engine, never a window.
    // (Handled inline below, after the host library loads.)
    let title = flag_value(&args, "--title").unwrap_or_else(|| "Aurora Hot Reload".to_string());
    let width: u32 = flag_value(&args, "--width").and_then(|s| s.parse().ok()).unwrap_or(500);
    let height: u32 = flag_value(&args, "--height").and_then(|s| s.parse().ok()).unwrap_or(560);
    let background = flag_value(&args, "--background").map(|s| parse_hex(&s)).unwrap_or(0xFFFFFF);

    // Map the shared runtime globally FIRST: every later image (host, UI) resolves aurora
    // symbols to this one instance instead of loading its own. Leaked, never closed:
    // dlclose ordering against thread-locals and per-image destructors segfaults on
    // exit, and a dev-loop launcher has no reason to unmap what the OS reclaims.
    let _shared = unsafe {
        match libloading::os::unix::Library::open(Some(&shared), libc::RTLD_NOW | libc::RTLD_GLOBAL) {
            Ok(lib) => {
                std::mem::forget(lib);
            }
            Err(e) => {
                eprintln!("hot-stub: cannot preload shared runtime {shared}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };
    // Reopen a handle for symbol visibility bookkeeping is unnecessary (forgotten above
    // stays mapped); load the host normally and leak it too on the selftest path.
    let host_lib = unsafe {
        match libloading::Library::new(&host) {
            Ok(lib) => lib,
            Err(e) => {
                eprintln!("hot-stub: cannot load host {host}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };

    // Headless verification mode: resolve the UI entry points here and hand the raw
    // pointers to the host's in-image self-test (which alone may execute them).
    if let Some(ui) = flag_value(&args, "--selftest") {
        let selftest: libloading::Symbol<HostSelftestFn> = unsafe {
            match host_lib.get(b"hot_host_selftest") {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("hot-stub: host {host} exports no hot_host_selftest: {e}");
                    return ExitCode::FAILURE;
                }
            }
        };
        let ui_lib = unsafe {
            match libloading::Library::new(&ui) {
                Ok(lib) => lib,
                Err(e) => {
                    eprintln!("hot-stub: cannot load ui {ui}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        };
        // Opaque addresses only: the stub never calls UI code (doing so through a
        // mismatched signature would be UB). The host transmutes them back to the real
        // signatures (same toolchain both sides) and executes them in-image.
        let (version_ptr, root_ptr): (usize, usize) = unsafe {
            let version: libloading::Symbol<*mut std::ffi::c_void> =
                match ui_lib.get(b"aurora_hot_version") {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("hot-stub: ui {ui} exports no aurora_hot_version: {e}");
                        return ExitCode::FAILURE;
                    }
                };
            let root: libloading::Symbol<*mut std::ffi::c_void> =
                match ui_lib.get(b"aurora_component_root") {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("hot-stub: ui {ui} exports no aurora_component_root: {e}");
                        return ExitCode::FAILURE;
                    }
                };
            (*version as usize, *root as usize)
        };
        let code = unsafe { selftest(version_ptr, root_ptr) };
        // Leak the images (see above): no dlclose ordering hazards on the way out.
        std::mem::forget(ui_lib);
        std::mem::forget(host_lib);
        return if code == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(code as u8)
        };
    }

    let Some(package) = flag_value(&args, "--package") else {
        usage()
    };
    let Some(root) = flag_value(&args, "--root") else {
        usage()
    };
    let run: libloading::Symbol<HostRunFn> = unsafe {
        match host_lib.get(b"hot_host_run") {
            Ok(run) => run,
            Err(e) => {
                eprintln!("hot-stub: host {host} exports no hot_host_run: {e}");
                return ExitCode::FAILURE;
            }
        }
    };

    let title = CString::new(title).unwrap_or_default();
    let package = CString::new(package).unwrap_or_default();
    let root = CString::new(root).unwrap_or_default();
    unsafe {
        run(
            title.as_ptr(),
            width,
            height,
            package.as_ptr(),
            root.as_ptr(),
            background,
        )
    };
    // The event loop only returns on exit: leak the images (see above) so teardown
    // cannot trip over dlclose ordering either.
    std::mem::forget(host_lib);
    ExitCode::SUCCESS
}
