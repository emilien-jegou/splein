// Single responsibility: In-window demo switcher — F5 grub-style menu and
// F7 engine cycling, all live in the current window, plus the external
// debug window.
//
// One engine, one runtime, one font database for the whole session (the
// same context the HUD draws in): F5 is a route change via
// `Engine::switch_root`, never an application-level reboot. The old tree's
// tickers retire and its hook state clears, so every visit mounts fresh
// while window, backend slot and debugger survive.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

use crate::app::debug_bus::{DebugBus, DebugCommand, NodeBrief, Snapshot};
use crate::app::debug_panel::{panel_icons, DebugPanel, PanelSync, PanelUi};
use crate::app::extension::AppExtension;
use crate::app::multibackend::{
    backend_display_name, MultiBackendConfig, BACKEND_ORDER,
};
use crate::app::runner::backend::{Backend, BackendFactory};
use crate::app::runner::clipboard::OsClipboardSync;
use crate::app::runner::frame_flow::present_one;
use crate::app::runner::pacing::{compute_jit_wake, compute_target_interval};
use crate::app::runner::pointer::PointerBridge;
use crate::app::VisualDebugger;
use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Color, Key as NodeKey};
use crate::input::CursorIcon;
use crate::reactive::{ambient_guard, Cx, ReactiveRuntime, Signal};
use crate::runtime::Engine;

/// Id of the grub-style menu demo (never listed in the menu itself).
pub const MENU_ID: &str = "__menu";

/// Smallest menu window that fits the vertical demo list without scrolling.
const MENU_MIN_W: u32 = 640;
const MENU_MIN_H: u32 = 880;

/// One switchable demo: title plus a boot function building its world.
///
/// `boot` receives the session runtime and returns the root element (built
/// under its own `"root"` hook scope), the background clear color, and the
/// demo's own extensions (physics drivers, tickers, monitors).
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

/// Menu interaction state, created once on the session runtime.
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
    /// Session reactive runtime shared by demos, menu and HUD.
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
    /// Runs the in-window switcher: F5 menu, F7 engine cycle, external panel.
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
}

/// Drops the live backend BEFORE creating its replacement.///
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

/// Debug window and its isolated engine. Everything panel-side lives here;
/// the only link to the app loop is the [`DebugBus`].
struct DebugWindows {
    window: Arc<Window>,
    engine: Engine,
    backend: Box<dyn Backend>,
    extensions: Vec<Box<dyn AppExtension>>,
    #[allow(dead_code)]
    runtime: Rc<RefCell<ReactiveRuntime>>,
    pending_resize: Option<(u32, u32)>,
    pointer: PointerBridge,
    cursor: CursorIcon,
}

/// Fixed debug panel size (matches the Paper design: 400pt rail).
const DEBUG_W: u32 = 400;
const DEBUG_H: u32 = 900;

/// Builds a [`NodeBrief`] for a keyed node, or `None` when it died.
fn brief_for(engine: &Engine, key: &NodeKey) -> Option<NodeBrief> {
    let id = engine.nodes().id(key.clone())?;
    engine.with_arena(|arena, _| {
        if !arena.is_valid(id) {
            return None;
        }
        let node = arena.get(id);
        let kind = match &node.kind {
            crate::tree::NodeKind::Group => "Group",
            crate::tree::NodeKind::Text(_) => "Text",
            crate::tree::NodeKind::Custom(_) => "Custom",
        }
        .to_string();
        let key_path = node
            .key
            .clone()
            .map(|k| k.0)
            .unwrap_or_default();
        let mut ancestors = Vec::new();
        let mut curr = arena.parent(id);
        while let Some(pid) = curr {
            if !arena.is_valid(pid) {
                break;
            }
            let p = arena.get(pid);
            ancestors.push(
                p.key
                    .clone()
                    .map(|k| k.0)
                    .unwrap_or_else(|| "<anon>".to_string()),
            );
            if ancestors.len() >= 8 {
                break;
            }
            curr = arena.parent(pid);
        }
        let r = engine.absolute_rect(key.clone()).unwrap_or_default();
        let fill = match &node.style.appearance.fill {
            Some(crate::foundation::Fill::Solid(c)) => Some(format!(
                "#{:02X}{:02X}{:02X}",
                (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                (c.b.clamp(0.0, 1.0) * 255.0) as u8
            )),
            _ => None,
        };
        Some(NodeBrief {
            key_path,
            kind,
            x: r.x,
            y: r.y,
            w: r.width,
            h: r.height,
            computed_w: format!("{:.0}px", r.width),
            computed_h: format!("{:.0}px", r.height),
            fill,
            ancestors,
        })
    })
}

fn demo_title_attr(title: &str) -> String {
    format!("{title}  [F5 demos | F7 engine | Esc exit]")
}

/// Mounts one demo on the session engine: a route change, not a reboot.
///
/// Engine, runtime, fonts and backend contexts all survive; only the tree,
/// background and demo-owned extensions swap. `switch_root` retires the old
/// tree's tickers, clears its hook state (fresh mount per visit) and
/// schedules a full repaint.
#[allow(clippy::too_many_arguments)]
fn mount_demo(
    demo: &DemoDef,
    meta: &[DemoMeta],
    menu: Option<MenuState>,
    engine: &mut Engine,
    extensions: &mut Vec<Box<dyn AppExtension>>,
    pointer: &mut PointerBridge,
    window: &Arc<Window>,
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    selected_key: &mut Option<NodeKey>,
) {
    // Prepare BEFORE building: bodies register tickers as they run, so the
    // epoch bump must precede the boot or it retires the new tree's own
    // clocks (frozen animation on every switch).
    engine.prepare_switch();
    let ctx = BootCtx {
        runtime: Rc::clone(runtime),
        engine_w: engine.logical_size().0,
        engine_h: engine.logical_size().1,
        demos: meta.to_vec(),
        menu,
    };
    let booted = (demo.boot)(&ctx);
    engine.switch_root(booted.root);
    engine.set_background(booted.background);
    extensions.truncate(1);
    extensions.extend(booted.extensions);
    *pointer = PointerBridge::new();
    // The old tree is gone: any picked/hovered node died with it.
    *selected_key = None;
    window.set_title(&demo_title_attr(demo.title));
    window.request_redraw();
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
        .unwrap_or_else(crate::app::runner::fail_display_init);
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

    let meta: Vec<DemoMeta> = demos
        .iter()
        .enumerate()
        .map(|(index, d)| DemoMeta { index, title: d.title })
        .collect();

    // Session runtime + engine: demos, menu and HUD share one font database
    // and one reactive graph for the whole session. Fonts load once; backend
    // text contexts cloned from here never go stale, so cross-engine glyph
    // misses are impossible by construction.
    let runtime: Rc<RefCell<ReactiveRuntime>> =
        config.runtime.unwrap_or_else(|| Rc::new(RefCell::new(ReactiveRuntime::new())));
    let mut current_id = initial_id.to_string();
    let mut extensions: Vec<Box<dyn AppExtension>>;
    let mut engine = {
        let demo = find_demo(&demos, &current_id);
        current_id = demo.id.to_string();
        let ctx = BootCtx {
            runtime: Rc::clone(&runtime),
            engine_w: config.width,
            engine_h: config.height,
            demos: meta.clone(),
            menu: None,
        };
        let booted = (demo.boot)(&ctx);
        let mut e = Engine::with_runtime(
            Rc::clone(&runtime),
            booted.root,
            config.width,
            config.height,
        );
        e.set_background(booted.background);
        e.load_font(include_bytes!("../../assets/inter-var.ttf"));
        let proxy = proxy.clone();
        e.runtime().borrow_mut().set_on_dirty(move || {
            let _ = proxy.send_event(());
        });
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = config.show_hud;
        // The panel lives in the external debug window: nothing HUD-shaped
        // is ever composited into the app scene.
        debugger.external_panel = true;
        extensions = vec![Box::new(debugger)];
        extensions.extend(booted.extensions);
        window.set_title(&demo_title_attr(demo.title));
        e
    };

    // Menu state lives on the session runtime once; reselected per open.
    let menu_state = MenuState {
        selected: Signal::new(Rc::clone(&runtime), 0usize),
        pending: Signal::new(Rc::clone(&runtime), None),
    };

    let mut backend: Option<Box<dyn Backend>> = Some({
        let physical = window.inner_size();
        let text_ctx = engine.text_context();
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
    });
    window.request_redraw();

    // Debug bus + external panel. The actuator keeps inspector overlays,
    // time control and capture in-app, but the HUD panel lives in the
    // separate debug window (external_panel), sharing nothing reactive.
    let (bus, cmd_rx) = DebugBus::new();
    if let Some(dbg) = extensions.iter_mut().find_map(|e| e.as_debugger()) {
        dbg.external_panel = true;
    }

    // Builds the debug window, engine and panel on demand (initially when
    // the panel starts open, later on F10 reopen). The debug engine is
    // fully isolated: own runtime, own fonts, own backend. Only snapshots
    // (plain data) cross over the bus.
    // Slot 0 is the CPU backend by runner convention (see run_demo):
    // the panel is simple UI and stays off the GPU.
    let cpu_factory = config.factories[0].expect("tiny-skia factory missing for panel");
    let open_debug_window = move |event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
                                      app_window: &Arc<Window>,
                                      bus: &DebugBus|
          -> Option<DebugWindows> {
        let icons = panel_icons();
        let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let ui = PanelUi::new(Rc::clone(&runtime), bus.clone(), icons);
        let cx = Cx::new(Rc::clone(&runtime));
        let root = cx
            .enter_keyed("panel", || DebugPanel(ui.clone()))
            .into_element();
        let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, DEBUG_W, DEBUG_H);
        engine.set_background(Color::hex(0x2C2C2C));
        engine.load_font(include_bytes!("../../assets/inter-var.ttf"));
        let extensions: Vec<Box<dyn AppExtension>> =
            vec![Box::new(PanelSync::new(bus.clone(), ui.version.clone()))];
        // Place right of the app window when its position is known.
        let pos = app_window
            .outer_position()
            .ok()
            .map(|p| {
                winit::dpi::LogicalPosition::new(
                    p.x as f64 + app_window.inner_size().width as f64 + 8.0,
                    p.y as f64,
                )
            });
        let mut builder = WindowBuilder::new()
            .with_title("Aurora — Debug")
            .with_inner_size(winit::dpi::LogicalSize::new(DEBUG_W, DEBUG_H))
            .with_resizable(true);
        if let Some(pos) = pos {
            builder = builder.with_position(pos);
        }
        let window = Arc::new(match builder.build(event_loop) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(error = %e, "debug window init failed");
                return None;
            }
        });
        let physical = window.inner_size();
        let backend = match cpu_factory(
            Arc::clone(&window),
            physical.width.max(1),
            physical.height.max(1),
            engine.text_context(),
        ) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = ?e, "debug backend init failed");
                return None;
            }
        };
        window.request_redraw();
        Some(DebugWindows {
            window,
            engine,
            backend,
            extensions,
            runtime,
            pending_resize: None,
            pointer: PointerBridge::new(),
            cursor: CursorIcon::Default,
        })
    };

    let mut debug: Option<DebugWindows> =
        if config.show_hud { open_debug_window(&event_loop, &window, &bus) } else { None };
    // Mirrors the panel window visibility (F10 / X / Esc in panel toggle it).
    let mut panel_visible = config.show_hud;
    // Set when the debug window wants another frame (interactions, snapshots).
    let mut dbg_redraw_pending = false;
    // Selection keys into the app tree (re-resolved per publish, so
    // purged nodes clear automatically). Hover resolves live per publish.
    // Pick arming comes from the panel.
    let mut selected_key: Option<NodeKey> = None;
    let mut pick_armed = false;
    // Last completed capture path, surfaced in the panel Changes tab.
    let mut capture_last: Option<String> = None;

    let mut menu: Option<OpenMenu> = None;
    let mut pending_resize: Option<(u32, u32)> = None;
    let mut pointer = PointerBridge::new();
    let mut cursor = CursorIcon::Default;
    let mut os_clipboard = OsClipboardSync::new();
    let factories: [Option<BackendFactory>; 4] = config.factories;
    // Frame pacing (mirrors the single-window runner): without it the loop
    // spins unthrottled whenever motion is active and the FPS readout
    // counts empty frames.
    let target_interval = compute_target_interval(config.fps_limit, &window);
    let mut redraw_pending = true;
    let mut last_present = Instant::now();
    let mut avg_compute = Duration::from_millis(2);

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } => {
                // Route to the debug window when it owns this event.
                let to_debug = debug
                    .as_ref()
                    .is_some_and(|d| d.window.id() == window_id);
                if to_debug {
                    // Guard the debug runtime: panel signals resolve against
                    // it even though the app engine framed last (or vice
                    // versa). Without this, any panel signal touch outside
                    // the debug engine's driven regions panics on epoch
                    // mismatch.
                    let rt = debug
                        .as_ref()
                        .map(|d| Rc::clone(&d.runtime))
                        .expect("debug routed without window");
                    let _dbg_guard = ambient_guard(&rt);
                    match event {
                        WindowEvent::KeyboardInput { event: key, .. }
                            if key.state == ElementState::Pressed
                                && key.logical_key == Key::Named(NamedKey::Escape) =>
                        {
                            // Esc in the panel closes it; the app keeps running.
                            if let Some(dbg) = debug.as_ref() {
                                dbg.window.set_visible(false);
                            }
                            panel_visible = false;
                        }
                        WindowEvent::CloseRequested => {
                            // X hides the panel (F10 reopens); the app keeps running.
                            if let Some(dbg) = debug.as_ref() {
                                dbg.window.set_visible(false);
                            }
                            panel_visible = false;
                        }
                        WindowEvent::Resized(size) => {
                            if let Some(dbg) = debug.as_mut() {
                                dbg.pending_resize =
                                    Some((size.width.max(1), size.height.max(1)));
                                dbg.window.request_redraw();
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if let Some(dbg) = debug.as_mut() {
                                let mut wants = false;
                                for ext in dbg.extensions.iter_mut() {
                                    wants |= ext.on_update(&mut dbg.engine);
                                }
                                present_one(
                                    &mut dbg.engine,
                                    &mut *dbg.backend,
                                    &mut dbg.extensions,
                                    dbg.pending_resize.take(),
                                );
                                dbg_redraw_pending = wants || dbg.engine.has_active_motion();
                            }
                        }
                        _ => {
                            if let Some(dbg) = debug.as_mut() {
                                if dbg.pointer.dispatch(&event, &mut dbg.engine) {
                                    dbg_redraw_pending = true;
                                    dbg.window.request_redraw();
                                }
                                let icon = dbg.engine.cursor_icon();
                                if icon != dbg.cursor {
                                    dbg.cursor = icon;
                                    dbg.window.set_cursor_icon(match icon {
                                        CursorIcon::Default => {
                                            winit::window::CursorIcon::Default
                                        }
                                        CursorIcon::Text => winit::window::CursorIcon::Text,
                                    });
                                }
                            }
                        }
                    }
                    return;
                }
                // Guard the session runtime for the whole app branch: menu
                // state, boots (whose derived values evaluate eagerly) and
                // extension touches all resolve session signals outside any
                // driven region here. Nesting the engine's own guards inside
                // frames/dispatch is harmless (same runtime).
                let _app_guard = ambient_guard(&runtime);
                // Window-level keys first: menu, combine, cycle, exit.
                if let WindowEvent::KeyboardInput { event: key, .. } = &event {
                    let switch = key.state == ElementState::Pressed
                        && handle_switcher_key(
                            &key.logical_key,
                            menu.is_some(),
                            meta.len(),
                            Some(menu_state.clone()),
                        )
                        .is_some();
                    if switch {
                        let next = handle_switcher_key(
                            &key.logical_key,
                            menu.is_some(),
                            meta.len(),
                            Some(menu_state.clone()),
                        )
                        .unwrap();
                        match next {
                            SwitchAction::OpenMenu => {
                                let physical = window.inner_size();
                                let prev = (physical.width.max(1), physical.height.max(1));
                                let target_w = prev.0.max(MENU_MIN_W);
                                let target_h = prev.1.max(MENU_MIN_H);
                                // Never resize the engine here: backend and
                                // engine must resize together inside
                                // present_one, or the backend surface goes
                                // stale (pre-sized engine makes present_one
                                // skip the backend resize → trimmed uploads →
                                // remnants). The pending resize pairs them on
                                // the next frame; the async Resized converges
                                // the window right after.
                                pending_resize = Some((target_w, target_h));
                                menu_state.selected.set(0);
                                menu_state.pending.set(None);
                                mount_demo(
                                    &menu_def,
                                    &meta,
                                    Some(menu_state.clone()),
                                    &mut engine,
                                    &mut extensions,
                                    &mut pointer,
                                    &window,
                                    &runtime,
                                    &mut selected_key,
                                );
                                menu = Some(OpenMenu {
                                    return_id: current_id.clone(),
                                    prev_size: prev,
                                });
                                bus.push_event("menu opened".to_string());
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
                                        &runtime,
                                        &mut selected_key,
                                    );
                                    bus.push_event(format!("back to {current_id}"));
                                }
                            }
                            SwitchAction::Launch(index) => {
                                menu.take();
                                {
                                    let demo =
                                        find_demo(&demos, meta_index_id(&meta, &demos, index, initial_id));
                                    // Launched demos inherit the roomy menu
                                    // window; they lay out fill-based.
                                    mount_demo(
                                        demo,
                                        &meta,
                                        None,
                                        &mut engine,
                                        &mut extensions,
                                        &mut pointer,
                                        &window,
                                        &runtime,
                                        &mut selected_key,
                                    );
                                    current_id = demo.id.to_string();
                                    bus.push_event(format!("opened {}", current_id));
                                }
                            }
                            SwitchAction::MoveSelection(delta) => {
                                menu_state.selected.update(|s| {
                                    let n = meta.len();
                                    *s = (*s as isize + delta).rem_euclid(n as isize)
                                        as usize
                                });
                                window.request_redraw();
                            }
                            SwitchAction::CycleEngine => {
                                single_slot = (single_slot + 1) % BACKEND_ORDER.len();
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
                                // Fresh canvas, live scene: repaint all, or
                                // the new backend uploads incremental damage
                                // from an undefined canvas (black tiles).
                                engine.request_full_repaint();
                                tracing::info!(
                                    engine = backend_display_name(
                                        BACKEND_ORDER[slot]
                                    ),
                                    "engine cycled"
                                );
                                bus.push_event(format!(
                                    "engine {}",
                                    backend_display_name(BACKEND_ORDER[slot])
                                ));
                                window.request_redraw();
                            }
                            SwitchAction::Exit => elwt.exit(),
                        }
                        return;
                    }
                }
                if pointer.dispatch(&event, &mut engine) {
                    redraw_pending = true;
                    window.request_redraw();
                }
                // Node picking: armed by the panel crosshair, the next app
                // click selects the hit node (or clears on empty space).
                if pick_armed {
                    if let WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        ..
                    } = &event
                    {
                        if let Some(pos) = engine.pointer_position() {
                            selected_key = engine.hit_test(pos.x, pos.y);
                            bus.push_event(
                                selected_key
                                    .as_ref()
                                    .map(|k| format!("selected {}", k.0))
                                    .unwrap_or_else(|| "selection cleared".to_string()),
                            );
                        } else {
                            selected_key = None;
                        }
                        pick_armed = false;
                    }
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
                    redraw_pending = true;
                    window.request_redraw();
                }
                // F10 toggles the external panel (visibility only; the
                // actuator keeps inspector/time/capture state regardless).
                if let Some(show) = extensions
                    .iter_mut()
                    .find_map(|ext| ext.as_debugger())
                    .map(|dbg| dbg.show_hud)
                {
                    if show != panel_visible {
                        panel_visible = show;
                        if show && debug.is_none() {
                            debug = open_debug_window(elwt, &window, &bus);
                        }
                        if let Some(dbg) = debug.as_ref() {
                            dbg.window.set_visible(show);
                            if show {
                                dbg.window.request_redraw();
                            }
                        }
                    }
                }
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(size) => {
                        pending_resize =
                            Some((size.width.max(1), size.height.max(1)));
                        redraw_pending = true;
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        // Panel commands drain before framing so time/scale
                        // changes apply to this frame, not the next.
                        for cmd in cmd_rx.try_iter() {
                            match &cmd {
                                DebugCommand::ArmPick => {
                                    pick_armed = true;
                                    bus.push_event("pick armed".to_string());
                                }
                                DebugCommand::ClearSelection => {
                                    selected_key = None;
                                }
                                _ => {
                                    if let Some(dbg) = extensions
                                        .iter_mut()
                                        .find_map(|ext| ext.as_debugger())
                                    {
                                        if dbg.apply_panel_command(&cmd, &mut engine) {
                                            redraw_pending = true;
                                        }
                                    }
                                }
                            }
                            if matches!(cmd, DebugCommand::Capture) {
                                bus.push_event("capture requested".to_string());
                            }
                        }
                        // Mouse-driven menu requests drain before framing.
                        if menu.is_some() {
                            let action = menu_state.pending.get();
                            if action.is_some() {
                                menu_state.pending.set(None);
                                match action {
                                    Some(MenuAction::Launch(index)) => {
                                        let demo = find_demo(
                                            &demos,
                                            meta_index_id(&meta, &demos, index, initial_id),
                                        );
                                        menu = None;
                                            mount_demo(
                                                demo,
                                                &meta,
                                                None,
                                                &mut engine,
                                                &mut extensions,
                                                &mut pointer,
                                                &window,
                                                &runtime,
                                                &mut selected_key,
                                            );
                                            current_id = demo.id.to_string();
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
                                                &runtime,
                                                &mut selected_key,
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
                        let was_capturing = extensions
                            .iter_mut()
                            .find_map(|ext| ext.as_debugger())
                            .is_some_and(|dbg| dbg.capture_armed());
                        let compute_dur = present_one(
                            &mut engine,
                            &mut **backend.as_mut().expect("backend present"),
                            &mut extensions,
                            pending_resize.take(),
                        );
                        if was_capturing {
                            if let Some(path) = extensions
                                .iter_mut()
                                .find_map(|ext| ext.as_debugger())
                                .and_then(|dbg| dbg.last_capture_path())
                            {
                                capture_last = Some(path.clone());
                                bus.push_event(format!("capture {path}"));
                            }
                        }
                        avg_compute =
                            avg_compute.mul_f32(0.8) + compute_dur.mul_f32(0.2);
                        last_present = Instant::now();
                        os_clipboard.sync(&engine);
                        redraw_pending = wants_redraw || engine.has_active_motion();
                        // Publish telemetry for the external panel, then wake
                        // it when visible. Plain data only — never layouts.
                        if panel_visible {
                            publish_snapshot(
                                &bus,
                                &mut extensions,
                                &engine,
                                backend.as_ref().map(|b| b.tag()),
                                &demos,
                                &current_id,
                                engine.pointer_position().and_then(|pos| {
                                    engine.hit_test(pos.x, pos.y)
                                }),
                                selected_key.clone(),
                                pick_armed,
                                capture_last.clone(),
                            );
                            if let Some(dbg) = debug.as_ref() {
                                dbg.window.request_redraw();
                                dbg_redraw_pending = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::UserEvent(()) => {
                redraw_pending = true;
                window.request_redraw();
                if panel_visible {
                    if let Some(dbg) = debug.as_ref() {
                        dbg.window.request_redraw();
                        dbg_redraw_pending = true;
                    }
                }
            }
            Event::AboutToWait => {
                if redraw_pending {
                    if let Some(interval) = target_interval {
                        let wake = compute_jit_wake(last_present, interval, avg_compute);
                        if Instant::now() >= wake {
                            window.request_redraw();
                        } else {
                            elwt.set_control_flow(ControlFlow::WaitUntil(wake));
                        }
                    } else {
                        window.request_redraw();
                    }
                } else if dbg_redraw_pending {
                    if let Some(dbg) = debug.as_ref() {
                        dbg.window.request_redraw();
                    }
                } else {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(500),
                    ));
                }
            }
            _ => {}
        })
        .expect("Event loop execution failed");
}

/// Fills and publishes one telemetry snapshot for the external panel.
/// Hover/selection keys re-resolve every publish, so purged nodes clear
/// automatically instead of dangling.
#[allow(clippy::too_many_arguments)]
fn publish_snapshot(
    bus: &DebugBus,
    extensions: &mut [Box<dyn AppExtension>],
    engine: &Engine,
    backend_tag: Option<&str>,
    demos: &[DemoDef],
    current_id: &str,
    hover_key: Option<NodeKey>,
    selected_key: Option<NodeKey>,
    pick_armed: bool,
    capture_last: Option<String>,
) {
    let Some(dbg) = extensions.iter_mut().find_map(|e| e.as_debugger()) else {
        return;
    };
    let diag = dbg.last_diagnostics();
    let title = demos
        .iter()
        .find(|d| d.id == current_id)
        .map(|d| d.title)
        .unwrap_or(current_id);
    bus.publish(Snapshot {
        version: 0,
        fps: dbg.display_fps(),
        fps_avg5: dbg.avg_fps_5s(),
        total_ms: diag.timings.total.as_secs_f32() * 1000.0,
        raster_ms: diag.timings.raster.as_secs_f32() * 1000.0,
        present_ms: diag.timings.present.as_secs_f32() * 1000.0,
        backend: backend_tag.unwrap_or("?").to_string(),
        inspector: dbg.mode,
        time_scale: dbg.time_scale(),
        frame_index: diag.frame_index,
        demo_id: current_id.to_string(),
        demo_title: title.to_string(),
        hovered: hover_key.as_ref().and_then(|k| brief_for(engine, k)),
        selected: selected_key.as_ref().and_then(|k| brief_for(engine, k)),
        focus_present: engine.focused_text().is_some(),
        motion_active: engine.has_active_motion(),
        pick_armed,
        events: bus.latest().map(|s| s.events).unwrap_or_default(),
        capture_last,
    });
}

/// Resolves a menu action index to a demo id (falls back to `initial_id`).
fn meta_index_id<'a>(
    meta: &[DemoMeta],
    demos: &[DemoDef],
    index: usize,
    initial_id: &'a str,
) -> &'a str {
    meta.get(index)
        .map(|m| demos[m.index].id)
        .unwrap_or(initial_id)
}

/// Closes the menu back to the demo it interrupted, restoring the window
/// size. The engine persists; only the tree swaps (backend + engine resize
/// pair up through the async Resized flow).
fn close_menu_to(
    open: &OpenMenu,
    demos: &[DemoDef],
    meta: &[DemoMeta],
    current_id: &mut String,
    engine: &mut Engine,
    extensions: &mut Vec<Box<dyn AppExtension>>,
    pointer: &mut PointerBridge,
    window: &Arc<Window>,
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    selected_key: &mut Option<NodeKey>,
) {
    let (pw, ph) = open.prev_size;
    let _ = window.request_inner_size(winit::dpi::LogicalSize::new(pw, ph));
    // No synchronous engine resize: see the pairing rule in OpenMenu.
    let demo = find_demo(demos, &open.return_id);
    mount_demo(
        demo, meta, None, engine, extensions, pointer, window, runtime, selected_key,
    );
    *current_id = demo.id.to_string();
}

enum SwitchAction {
    OpenMenu,
    CloseMenu,
    Launch(usize),
    MoveSelection(isize),
    CycleEngine,
    Exit,
}

/// Switcher keys: F5 menu, F7 engine cycle, Esc exit/back.
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
