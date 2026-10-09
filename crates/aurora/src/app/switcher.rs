// Single responsibility: In-window demo switcher — F5 grub-style menu, F6
// quad combine, F7 engine cycle, all live in the current window.
//
// Every example binary hosts every demo: each switch rebuilds a fresh Engine
// (fresh reactive runtime, so no hook/ticker state leaks across demos) while
// the window, backend slot selection and debugger survive. F6 swaps the
// single backend for the quad-combine backend in the same window (resizing
// it to fit quadrants); F7 cycles single raster engines.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::EventLoopBuilder;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

use crate::app::extension::AppExtension;
use crate::app::multibackend::{
    backend_display_name, quad_layout, MultiBackendConfig, BACKEND_ORDER, QUAD_STRIP,
};
use crate::app::runner::backend::{Backend, BackendFactory};
use crate::app::runner::clipboard::OsClipboardSync;
use crate::app::runner::frame_flow::present_one;
use crate::app::runner::pointer::PointerBridge;
use crate::app::VisualDebugger;
use crate::dsl::element::Element;
use crate::foundation::Color;
use crate::input::CursorIcon;
use crate::reactive::{ReactiveRuntime, Signal};
use crate::runtime::Engine;

/// Id of the grub-style menu demo (never listed in the menu itself).
pub const MENU_ID: &str = "__menu";

/// Smallest menu window that fits the demo list without scrolling.
const MENU_MIN_W: u32 = 1000;
const MENU_MIN_H: u32 = 760;

/// One switchable demo: title plus a boot function rebuilding its world.
///
/// `boot` receives a fresh runtime per switch and returns the root element
/// (built under its own `"root"` hook scope), the background clear color,
/// and the demo's own extensions (physics drivers, tickers, monitors).
/// Per-demo extensions are installed on switch and dropped on leave, so a
/// ticker that writes every frame can never pin the loop while its demo is
/// gone.
#[derive(Clone, Copy)]
pub struct DemoDef {
    /// Stable id named by the owning example binary.
    pub id: &'static str,
    /// Menu row title.
    pub title: &'static str,
    /// Rebuilds root + background + extensions on a fresh runtime.
    pub boot: fn(&BootCtx) -> BootedDemo,
}

/// Title list handed to the menu boot (all demos except the menu itself).
#[derive(Clone)]
pub struct DemoMeta {
    /// Index into the registry (menu actions reference this).
    pub index: usize,
    /// Row title.
    pub title: &'static str,
}

/// Menu interaction state, recreated on a fresh runtime every time the menu opens.
#[derive(Clone)]
pub struct MenuState {
    /// Highlighted row.
    pub selected: Signal<usize>,
    /// Mouse-driven request drained by the runner before the next frame.
    pub pending: Signal<Option<MenuAction>>,
}

/// Mouse request from a menu row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Switch to registry index live.
    Launch(usize),
    /// Close the menu back to the previous demo.
    Back,
}

/// What a demo boot receives.
pub struct BootCtx {
    /// Fresh reactive runtime for this mount.
    pub runtime: Rc<RefCell<ReactiveRuntime>>,
    /// Engine size the root will lay out at.
    pub engine_w: u32,
    /// Engine size the root will lay out at.
    pub engine_h: u32,
    /// All switchable demos (menu excluded) for the menu boot.
    pub demos: Vec<DemoMeta>,
    /// Present only when booting the menu itself.
    pub menu: Option<MenuState>,
}

/// What a demo boot returns.
pub struct BootedDemo {
    /// Root element, already built under its hook scope.
    pub root: Element,
    /// Background clear color.
    pub background: Color,
    /// Demo-owned extensions, active only while mounted.
    pub extensions: Vec<Box<dyn AppExtension>>,
}

impl MultiBackendConfig {
    /// Runs the in-window switcher: F5 menu, F6 quad, F7 engine cycle.
    ///
    /// `demos` is the full registry (menu excluded), `menu` the menu page
    /// boot, `initial_id` the demo this binary starts at.
    pub fn run_switchable(self, demos: Vec<DemoDef>, menu: DemoDef, initial_id: &'static str) {
        run_switchable_loop(self, demos, menu, initial_id);
    }
}

struct OpenMenu {
    return_id: String,
    prev_size: (u32, u32),
    was_quad: bool,
    state: MenuState,
}

/// Drops the live backend BEFORE creating its replacement.
///
/// wgpu/vello open one swapchain surface per window, and the platform
/// rejects a second surface on the same window while the first is alive
/// ("Surface already has a fifo" / Invalid surface on configure).
/// Creation order is load-bearing: destroy, then create.
fn replace_backend(
    backend: &mut Option<Box<dyn Backend>>,
    window: &Arc<Window>,
    factory: Option<BackendFactory>,
    w: u32,
    h: u32,
    text_ctx: crate::text::TextContext,
    what: &str,
) {
    drop(backend.take());
    if let Some(f) = factory {
        match f(Arc::clone(window), w, h, text_ctx) {
            Ok(b) => *backend = Some(b),
            Err(e) => tracing::error!(error = ?e, what, "backend rebuild failed"),
        }
    }
}

fn find_demo<'d>(demos: &'d [DemoDef], id: &str) -> &'d DemoDef {
    match demos.iter().find(|d| d.id == id) {
        Some(d) => d,
        None => {
            tracing::warn!(id, "unknown demo id, falling back to first");
            &demos[0]
        }
    }
}

fn build_engine(
    demo: &DemoDef,
    ctx: &BootCtx,
    proxy: &winit::event_loop::EventLoopProxy<()>,
) -> (Engine, Vec<Box<dyn AppExtension>>) {
    let booted = (demo.boot)(ctx);
    let mut engine =
        Engine::with_runtime(Rc::clone(&ctx.runtime), booted.root, ctx.engine_w, ctx.engine_h);
    engine.set_background(booted.background);
    engine.load_font(include_bytes!("../../assets/inter-var.ttf"));
    let proxy = proxy.clone();
    engine.runtime().borrow_mut().set_on_dirty(move || {
        let _ = proxy.send_event(());
    });
    (engine, booted.extensions)
}

fn demo_title_attr(title: &str) -> String {
    format!("{title}  [F5 demos | F6 quad | F7 engine | Esc exit]")
}

#[allow(clippy::too_many_lines)]
fn run_switchable_loop(
    config: MultiBackendConfig,
    demos: Vec<DemoDef>,
    menu_def: DemoDef,
    initial_id: &'static str,
) {
    assert!(!demos.is_empty(), "switcher needs at least one demo");
    assert!(
        !demos.iter().any(|d| d.id == MENU_ID),
        "demo id {MENU_ID} is reserved for the menu"
    );
    let event_loop = EventLoopBuilder::with_user_event()
        .build()
        .expect("Display init failed");
    let proxy = event_loop.create_proxy();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(initial_id)
            .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
            .with_fullscreen(
                config
                    .fullscreen
                    .then(|| winit::window::Fullscreen::Borderless(None)),
            )
            .with_resizable(true)
            .build(&event_loop)
            .expect("window init failed"),
    );

    let initial_slot = BACKEND_ORDER
        .iter()
        .position(|b| *b == config.initial)
        .unwrap_or(0);
    let mut single_slot = initial_slot;
    let mut is_quad = config.combine_on_boot;

    let meta: Vec<DemoMeta> = demos
        .iter()
        .enumerate()
        .map(|(index, d)| DemoMeta { index, title: d.title })
        .collect();

    // First engine at demo size, fonts loaded before any backend exists so
    // every backend's text context shares the loaded fonts.
    let mut current_id = initial_id.to_string();
    let mut extensions: Vec<Box<dyn AppExtension>>;
    let mut engine = {
        let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let demo = find_demo(&demos, &current_id);
        current_id = demo.id.to_string();
        let ctx = BootCtx {
            runtime,
            engine_w: config.width,
            engine_h: config.height,
            demos: meta.clone(),
            menu: None,
        };
        let (e, exts) = build_engine(demo, &ctx, &proxy);
        let mut all: Vec<Box<dyn AppExtension>> = Vec::with_capacity(1 + exts.len());
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = config.show_hud;
        all.push(Box::new(debugger));
        all.extend(exts);
        extensions = all;
        window.set_title(&demo_title_attr(demo.title));
        e
    };

    let mut backend: Option<Box<dyn Backend>> = Some({
        let physical = window.inner_size();
        let text_ctx = engine.text_context();
        if is_quad {
            let (qw, qh) = (config.width, config.height);
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(
                qw * 2,
                qh * 2 + QUAD_STRIP,
            ));
            match config.quad_factory {
                Some(qf) => {
                    let text_ctx = engine.text_context();
                    match qf(Arc::clone(&window), qw * 2, qh * 2 + QUAD_STRIP, text_ctx) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::error!(error = ?e, "quad boot failed, single tiny-skia instead");
                            is_quad = false;
                            single_slot = 0;
                            let text_ctx = engine.text_context();
                            let physical = window.inner_size();
                            config.factories[0]
                                .expect("tiny-skia factory missing")(
                                Arc::clone(&window),
                                physical.width.max(1),
                                physical.height.max(1),
                                text_ctx,
                            )
                            .expect("tiny-skia backend init failed")
                        }
                    }
                }
                None => panic!("--combine without quad factory"),
            }
        } else {
            let (bw, bh) = (physical.width.max(1), physical.height.max(1));
            match config.factories[single_slot] {
                Some(f) => f(Arc::clone(&window), bw, bh, text_ctx).unwrap_or_else(|e| {
                    tracing::error!(error = ?e, "initial backend failed, falling back to tiny-skia");
                    single_slot = 0;
                    config.factories[0]
                        .expect("tiny-skia factory missing")(
                        Arc::clone(&window), bw, bh, engine.text_context(),
                    )
                    .expect("tiny-skia backend init failed")
                }),
                None => panic!("no factory for initial backend"),
            }
        }
    });
    window.request_redraw();

    let mut menu: Option<OpenMenu> = None;
    let mut pending_resize: Option<(u32, u32)> = None;
    let mut pointer = PointerBridge::new();
    let mut cursor = CursorIcon::Default;
    let mut os_clipboard = OsClipboardSync::new();
    let factories: [Option<BackendFactory>; 4] = config.factories;
    let quad_factory: Option<BackendFactory> = config.quad_factory;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event, .. } => {
                // Window-level keys first: menu, combine, cycle, exit.
                if let WindowEvent::KeyboardInput { event: key, .. } = &event {
                    let switch = key.state == ElementState::Pressed
                        && handle_switcher_key(
                            &key.logical_key,
                            menu.is_some(),
                            meta.len(),
                            menu.as_ref().map(|m| m.state.clone()),
                        )
                        .is_some();
                    if switch {
                        let next = handle_switcher_key(
                            &key.logical_key,
                            menu.is_some(),
                            meta.len(),
                            menu.as_ref().map(|m| m.state.clone()),
                        )
                        .unwrap();
                        match next {
                            SwitchAction::OpenMenu => {
                                let was_quad = is_quad;
                                if was_quad {
                                    // The menu lays out 1:1: drop quad first.
                                    is_quad = false;
                                    let tc = engine.text_context();
                                    let (ew, eh) = engine.logical_size();
                                    replace_backend(
                                        &mut backend,
                                        &window,
                                        factories[single_slot % BACKEND_ORDER.len()],
                                        ew,
                                        eh,
                                        tc,
                                        "unquad for menu",
                                    );
                                }
                                let physical = window.inner_size();
                                let prev = (physical.width.max(1), physical.height.max(1));
                                let target_w = prev.0.max(MENU_MIN_W);
                                let target_h = prev.1.max(MENU_MIN_H);
                                // Build the menu at the CURRENT engine size;
                                // the async Resized below grows backend and
                                // engine together through the standard flow.
                                // Pre-sizing the engine here is what raced
                                // the backend surface and panicked the
                                // presenter on mismatched buffers.
                                let (ew, eh) = engine.logical_size();
                                let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
                                let state = MenuState {
                                    selected: Signal::new(Rc::clone(&runtime), 0usize),
                                    pending: Signal::new(Rc::clone(&runtime), None),
                                };
                                let ctx = BootCtx {
                                    runtime,
                                    engine_w: ew,
                                    engine_h: eh,
                                    demos: meta.clone(),
                                    menu: Some(state.clone()),
                                };
                                let (fresh, exts) = build_engine(&menu_def, &ctx, &proxy);
                                engine = fresh;
                                extensions.truncate(1);
                                extensions.extend(exts);
                                pointer = PointerBridge::new();
                                menu = Some(OpenMenu {
                                    return_id: current_id.clone(),
                                    prev_size: prev,
                                    was_quad,
                                    state,
                                });
                                window.set_title(
                                    "Aurora — demos  [↑↓ move | Enter open | Esc back]",
                                );
                                let _ = window.request_inner_size(
                                    winit::dpi::LogicalSize::new(target_w, target_h),
                                );
                                window.request_redraw();
                            }
                            SwitchAction::CloseMenu => {
                                if let Some(open) = menu.take() {
                                    close_menu_to(
                                        &open,
                                        &demos,
                                        &meta,
                                        &mut current_id,
                                        &mut engine,
                                        &mut extensions,
                                        &mut pointer,
                                        &window,
                                        &proxy,
                                        &factories,
                                        single_slot,
                                        quad_factory,
                                        &mut backend,
                                        &mut is_quad,
                                    );
                                }
                            }
                            SwitchAction::Launch(index) => {
                                menu.take();
                                {
                                    let id = meta
                                        .get(index)
                                        .map(|m| demos[m.index].id)
                                        .unwrap_or(initial_id);
                                    // Launched demos inherit the roomy menu
                                    // window; they lay out fill-based.
                                    let physical = window.inner_size();
                                    let (ew, eh) =
                                        (physical.width.max(1), physical.height.max(1));
                                    current_id = swap_demo(
                                        &demos,
                                        &meta,
                                        None,
                                        id,
                                        ew,
                                        eh,
                                        &mut engine,
                                        &mut extensions,
                                        &mut pointer,
                                        &window,
                                        &proxy,
                                    );
                                }
                            }
                            SwitchAction::MoveSelection(delta) => {
                                if let Some(open) = menu.as_ref() {
                                    let n = meta.len();
                                    open.state.selected.update(|s| {
                                        *s = (*s as isize + delta).rem_euclid(n as isize)
                                            as usize
                                    });
                                    window.request_redraw();
                                }
                            }
                            SwitchAction::ToggleQuad => {
                                let (ew, eh) = engine.logical_size();
                                is_quad = !is_quad;
                                if is_quad {
                                    // Quad is headless-GPU + softbuffer: no
                                    // extra swapchain surface, but still drop
                                    // the single backend's surface first.
                                    drop(backend.take());
                                    let tc = engine.text_context();
                                    match quad_factory {
                                        Some(qf) => match qf(
                                            Arc::clone(&window),
                                            ew * 2,
                                            eh * 2 + QUAD_STRIP,
                                            tc,
                                        ) {
                                            Ok(b) => {
                                                backend = Some(b);
                                                let _ = window.request_inner_size(
                                                    winit::dpi::LogicalSize::new(
                                                        ew * 2,
                                                        eh * 2 + QUAD_STRIP,
                                                    ),
                                                );
                                                tracing::info!("quad combine on");
                                            }
                                            Err(e) => {
                                                tracing::error!(%e, "quad backend failed");
                                                is_quad = false;
                                            }
                                        },
                                        None => {
                                            tracing::warn!("no quad factory supplied");
                                            is_quad = false;
                                        }
                                    }
                                    if !is_quad {
                                        // Quad failed: restore the single backend.
                                        let tc = engine.text_context();
                                        replace_backend(
                                            &mut backend,
                                            &window,
                                            factories[single_slot % BACKEND_ORDER.len()],
                                            ew,
                                            eh,
                                            tc,
                                            "quad fallback",
                                        );
                                    }
                                } else {
                                    let tc = engine.text_context();
                                    let slot = single_slot % BACKEND_ORDER.len();
                                    replace_backend(
                                        &mut backend,
                                        &window,
                                        factories[slot],
                                        ew,
                                        eh,
                                        tc,
                                        "unquad",
                                    );
                                    let _ = window.request_inner_size(
                                        winit::dpi::LogicalSize::new(ew, eh),
                                    );
                                    tracing::info!("quad combine off");
                                }
                                window.request_redraw();
                            }
                            SwitchAction::CycleEngine => {
                                single_slot = (single_slot + 1) % BACKEND_ORDER.len();
                                if is_quad {
                                    is_quad = false;
                                    let (ew, eh) = engine.logical_size();
                                    let _ = window.request_inner_size(
                                        winit::dpi::LogicalSize::new(ew, eh),
                                    );
                                }
                                let tc = engine.text_context();
                                let (ew, eh) = engine.logical_size();
                                let slot = single_slot;
                                replace_backend(
                                    &mut backend,
                                    &window,
                                    factories[slot],
                                    ew,
                                    eh,
                                    tc,
                                    "engine cycle",
                                );
                                tracing::info!(
                                    engine = backend_display_name(
                                        BACKEND_ORDER[slot]
                                    ),
                                    "engine cycled"
                                );
                                window.request_redraw();
                            }
                            SwitchAction::Exit => elwt.exit(),
                        }
                        return;
                    }
                }
                if pointer.dispatch(&event, &mut engine) {
                    window.request_redraw();
                }
                let icon = engine.cursor_icon();
                if icon != cursor {
                    cursor = icon;
                    window.set_cursor_icon(match icon {
                        CursorIcon::Default => winit::window::CursorIcon::Default,
                        CursorIcon::Text => winit::window::CursorIcon::Text,
                    });
                }
                if extensions.iter_mut().any(|ext| ext.on_event(&event)) {
                    window.request_redraw();
                }
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(size) => {
                        let (w, h) = (size.width.max(1), size.height.max(1));
                        if is_quad {
                            // Quad carries the engine at quadrant size;
                            // present_one must not see the full size.
                            backend
                                .as_mut()
                                .expect("quad backend present")
                                .resize(w, h);
                            let (qw, qh) = quad_layout(w, h);
                            if (qw, qh) != engine.logical_size() {
                                engine.resize(qw, qh);
                            }
                        } else {
                            pending_resize = Some((w, h));
                        }
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        // Mouse-driven menu requests drain before framing.
                        if menu.is_some() {
                            let action = menu
                                .as_ref()
                                .and_then(|m| m.state.pending.get());
                            if action.is_some() {
                                if let Some(open) = menu.as_ref() {
                                    open.state.pending.set(None);
                                }
                                match action {
                                    Some(MenuAction::Launch(index)) => {
                                        let id = meta
                                            .get(index)
                                            .map(|m| demos[m.index].id)
                                            .unwrap_or(initial_id);
                                        menu = None;
                                        let physical = window.inner_size();
                                        let (ew, eh) =
                                            (physical.width.max(1), physical.height.max(1));
                                        current_id = swap_demo(
                                            &demos,
                                            &meta,
                                            None,
                                            id,
                                            ew,
                                            eh,
                                            &mut engine,
                                            &mut extensions,
                                            &mut pointer,
                                            &window,
                                            &proxy,
                                        );
                                    }
                                    Some(MenuAction::Back) => {
                                        if let Some(open) = menu.take() {
                                            close_menu_to(
                                                &open,
                                                &demos,
                                                &meta,
                                                &mut current_id,
                                                &mut engine,
                                                &mut extensions,
                                                &mut pointer,
                                                &window,
                                                &proxy,
                                                &factories,
                                                single_slot,
                                                quad_factory,
                                                &mut backend,
                                                &mut is_quad,
                                            );
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
                        let mut wants_redraw = false;
                        for ext in extensions.iter_mut() {
                            wants_redraw |= ext.on_update(&mut engine);
                        }
                        present_one(
                            &mut engine,
                            &mut **backend.as_mut().expect("backend present"),
                            &mut extensions,
                            pending_resize.take(),
                        );
                        os_clipboard.sync(&engine);
                        if wants_redraw || engine.has_active_motion() {
                            window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            Event::UserEvent(()) => {
                window.request_redraw();
            }
            Event::AboutToWait => {}
            _ => {}
        })
        .expect("Event loop execution failed");
}

/// Rebuilds the engine for demo `id` at `(ew, eh)`, swapping per-demo
/// extensions. Window, backend and debugger survive; pointer state resets.
#[allow(clippy::too_many_arguments)]
fn swap_demo(
    demos: &[DemoDef],
    meta: &[DemoMeta],
    menu: Option<MenuState>,
    id: &str,
    ew: u32,
    eh: u32,
    engine: &mut Engine,
    extensions: &mut Vec<Box<dyn AppExtension>>,
    pointer: &mut PointerBridge,
    window: &Arc<Window>,
    proxy: &winit::event_loop::EventLoopProxy<()>,
) -> String {
    let demo = find_demo(demos, id);
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let ctx = BootCtx {
        runtime,
        engine_w: ew,
        engine_h: eh,
        demos: meta.to_vec(),
        menu,
    };
    let (fresh, exts) = build_engine(demo, &ctx, proxy);
    *engine = fresh;
    extensions.truncate(1);
    extensions.extend(exts);
    *pointer = PointerBridge::new();
    window.set_title(&demo_title_attr(demo.title));
    window.request_redraw();
    demo.id.to_string()
}

/// Closes the menu back to the demo it interrupted, restoring size and (if
/// it was open) the quad backend.
#[allow(clippy::too_many_arguments)]
fn close_menu_to(
    open: &OpenMenu,
    demos: &[DemoDef],
    meta: &[DemoMeta],
    current_id: &mut String,
    engine: &mut Engine,
    extensions: &mut Vec<Box<dyn AppExtension>>,
    pointer: &mut PointerBridge,
    window: &Arc<Window>,
    proxy: &winit::event_loop::EventLoopProxy<()>,
    factories: &[Option<BackendFactory>; 4],
    single_slot: usize,
    quad_factory: Option<BackendFactory>,
    backend: &mut Option<Box<dyn Backend>>,
    is_quad: &mut bool,
) {
    let (pw, ph) = open.prev_size;
    if open.was_quad {
        *is_quad = true;
        let (qw, qh) = quad_layout(pw, ph);
        // Drop the menu's single surface before the quad backend is built.
        drop(backend.take());
        let mut quad_ok = false;
        if let Some(qf) = quad_factory {
            let tc = engine.text_context();
            match qf(Arc::clone(window), pw, ph, tc) {
                Ok(b) => {
                    *backend = Some(b);
                    quad_ok = true;
                }
                Err(e) => tracing::error!(%e, "quad rebuild failed"),
            }
        }
        if !quad_ok {
            // Fall back to single: engine goes full-size, backend rebuilt.
            *is_quad = false;
            let tc = engine.text_context();
            replace_backend(
                backend,
                window,
                factories[single_slot % BACKEND_ORDER.len()],
                pw,
                ph,
                tc,
                "menu-close quad fallback",
            );
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(pw, ph));
            *current_id = swap_demo(
                demos, meta, None, &open.return_id, pw, ph, engine, extensions, pointer,
                window, proxy,
            );
            return;
        }
        let _ = window.request_inner_size(winit::dpi::LogicalSize::new(pw, ph));
        *current_id = swap_demo(
            demos, meta, None, &open.return_id, qw, qh, engine, extensions, pointer,
            window, proxy,
        );
    } else {
        *is_quad = false;
        let _ = window.request_inner_size(winit::dpi::LogicalSize::new(pw, ph));
        *current_id = swap_demo(
            demos, meta, None, &open.return_id, pw, ph, engine, extensions, pointer,
            window, proxy,
        );
    }
}

/// What a handled switcher key asks the loop to do.
enum SwitchAction {
    OpenMenu,
    CloseMenu,
    Launch(usize),
    MoveSelection(isize),
    ToggleQuad,
    CycleEngine,
    Exit,
}

/// Switcher keys: F5 menu, F6 quad combine, F7 engine cycle, Esc exit/back.
/// Menu navigation (Up/Down/Home/End/Enter/Space) only while the menu is open.
fn handle_switcher_key(
    key: &Key,
    menu_open: bool,
    menu_len: usize,
    menu_state: Option<MenuState>,
) -> Option<SwitchAction> {
    match key {
        Key::Named(NamedKey::F5) => Some(if menu_open {
            SwitchAction::CloseMenu
        } else {
            SwitchAction::OpenMenu
        }),
        Key::Named(NamedKey::Escape) => Some(if menu_open {
            SwitchAction::CloseMenu
        } else {
            SwitchAction::Exit
        }),
        Key::Named(NamedKey::F6) if !menu_open => Some(SwitchAction::ToggleQuad),
        Key::Named(NamedKey::F7) if !menu_open => Some(SwitchAction::CycleEngine),
        Key::Named(NamedKey::ArrowDown) if menu_open => {
            Some(SwitchAction::MoveSelection(1))
        }
        Key::Named(NamedKey::ArrowUp) if menu_open => {
            Some(SwitchAction::MoveSelection(-1))
        }
        Key::Named(NamedKey::Home) if menu_open => {
            if let Some(st) = menu_state {
                st.selected.set(0);
            }
            None
        }
        Key::Named(NamedKey::End) if menu_open => {
            if let Some(st) = menu_state {
                st.selected.set(menu_len.saturating_sub(1));
            }
            None
        }
        Key::Named(NamedKey::Enter) if menu_open => menu_state
            .map(|st| SwitchAction::Launch(st.selected.get().min(menu_len.saturating_sub(1)))),
        Key::Character(c) if menu_open && c.as_str() == " " => menu_state
            .map(|st| SwitchAction::Launch(st.selected.get().min(menu_len.saturating_sub(1)))),
        _ => None,
    }
}
