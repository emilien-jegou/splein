// Responsibility: Headless verification suite for hot-reload components and runtime state preservation.

use riode::hot_reload::{
    component_abi_fingerprint, ComponentRootFn, ComponentVersionFn, SharedFactory,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn check(cond: bool, what: &str) -> Result<(), i32> {
    if cond {
        Ok(())
    } else {
        eprintln!("hot selftest FAILED: {what}");
        Err(1)
    }
}

#[no_mangle]
pub extern "Rust" fn devkit_selftest(version_fn: usize, root_fn: usize) -> i32 {
    let outcome = std::panic::catch_unwind(|| run_selftest(version_fn, root_fn));
    match outcome {
        Ok(code) => code,
        Err(_) => {
            eprintln!("hot selftest: panicked; see logs above");
            1
        }
    }
}

fn run_selftest(version_fn: usize, root_fn: usize) -> i32 {
    let version_fn: ComponentVersionFn = unsafe { std::mem::transmute(version_fn) };
    let root_fn: ComponentRootFn = unsafe { std::mem::transmute(root_fn) };

    let test = || -> Result<(), i32> {
        check(version_fn() == component_abi_fingerprint(), "protocol handshake agrees")?;

        let runtime = Rc::new(RefCell::new(riode::reactive::ReactiveRuntime::new()));
        runtime.borrow_mut().begin_build_window();
        let cx = riode::reactive::Cx::new(Rc::clone(&runtime));
        let current = Rc::new(Cell::new(Some(root_fn)));
        let factory: SharedFactory = {
            let current = Rc::clone(&current);
            Rc::new(move || current.get().expect("component entry installed")())
        };

        let tree = cx.enter_keyed("root", || factory());
        let mut engine = riode::runtime::Engine::with_runtime(Rc::clone(&runtime), tree, 500, 560);
        runtime.borrow_mut().end_build_window(true);
        engine.set_root_factory(Rc::clone(&factory));
        engine.advance(1.0 / 60.0);
        engine.frame();

        check(engine.hot_reload().registry().contains("hot_ui::Counter"), "scopes tracked")?;

        let (x, y) = engine.rect("counter").map(|r| (r.x + r.width / 2.0, r.y + r.height / 2.0)).expect("mounted");
        engine.queue_pointer_down(x, y);
        engine.advance(1.0 / 60.0);
        engine.frame();
        engine.queue_pointer_up(x, y);
        engine.advance(1.0 / 60.0);
        engine.frame();

        let tapped = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check((tapped - 120.0).abs() < 0.5, "tap lands in dylib hook state")?;

        current.set(Some(root_fn));
        engine.queue_full_reload();
        engine.advance(1.0 / 60.0);
        engine.frame();

        let kept = engine.rect("counter").map(|r| r.height).unwrap_or(-1.0);
        check((kept - 120.0).abs() < 0.5, "state survives code swap")?;

        Ok(())
    };

    match test() {
        Ok(()) => {
            eprintln!("hot selftest: all checks passed");
            0
        }
        Err(code) => code,
    }
}
