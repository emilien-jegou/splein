// Single responsibility: Multi-backend demo runner — one shared Engine presented
// through one of five raster backends (1-5 select, Tab/F7 cycle).
//
// One `Engine` (one reactive runtime, one `Scene`) frames once; each live
// backend presents that same scene into its own OS window. Backend switching
// reuses cached backends (lazy init, fail-soft per backend).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smallvec::SmallVec;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::app::runner::backend::{
    Backend, BackendFactory, DamageResolver, FramePostProcess, Present, SurfaceRequest,
};
use crate::app::runner::pointer::PointerBridge;
use crate::app::runner::{clipboard::OsClipboardSync, FpsLimit};
use crate::app::VisualDebugger;
use crate::dsl::component::IntoElement;
use crate::foundation::{Color, DamageRegion, ResolvedRect};
use crate::hot_reload::SharedFactory;
use crate::input::CursorIcon;
use crate::reactive::{Cx, ReactiveRuntime};
use crate::render::RenderBackend;
use crate::runtime::Engine;

/// Ordered backend slots: index selects, Tab/F7 cycles.
pub const BACKEND_ORDER: [RenderBackend; 5] = [
    RenderBackend::TinySkia,
    RenderBackend::Vello,
    RenderBackend::VelloCpu,
    RenderBackend::Wgpu,
    RenderBackend::Skia,
];

/// Descriptive display name per backend slot.
pub fn backend_display_name(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::TinySkia => "tiny-skia",
        RenderBackend::Vello => "vello-gpu",
        RenderBackend::VelloCpu => "vello-cpu",
        RenderBackend::Wgpu => "wgpu",
        RenderBackend::Skia => "skia",
    }
}

/// Focused backend slot: `1-5` select, Tab/F7 cycles round-robin.
pub type BackendSlot = usize;

/// Next slot in Tab/F7 cycle order.
pub fn cycle_slot(slot: BackendSlot) -> BackendSlot {
    (slot + 1) % BACKEND_ORDER.len()
}

/// Trailing-5s per-backend present stats for single-mode surfaces.
#[allow(dead_code)]
pub struct BackendStats {
    stamps: VecDeque<Instant>,
    last_raster: Duration,
    last_present: Duration,
    frames: u64,
}

impl BackendStats {
    fn new() -> Self {
        Self {
            stamps: VecDeque::with_capacity(600),
            last_raster: Duration::ZERO,
            last_present: Duration::ZERO,
            frames: 0,
        }
    }

    fn record(&mut self, raster: Duration, present: Duration) {
        let now = Instant::now();
        self.stamps.push_back(now);
        while let Some(front) = self.stamps.front() {
            if now.duration_since(*front) > Duration::from_secs(6) {
                self.stamps.pop_front();
            } else {
                break;
            }
        }
        self.last_raster = raster;
        self.last_present = present;
        self.frames += 1;
    }

    /// Average FPS over the trailing 5s window.
    pub fn avg_fps_5s(&self) -> f32 {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(5);
        let mut count = 0u32;
        let mut oldest: Option<Instant> = None;
        for t in &self.stamps {
            if *t >= cutoff {
                if oldest.is_none() {
                    oldest = Some(*t);
                }
                count += 1;
            }
        }
        match (count, oldest) {
            (0 | 1, _) => 0.0,
            (_, Some(o)) => {
                let dt = (now - o).as_secs_f32().max(0.5);
                (count as f32 / dt).min(9999.0)
            }
            _ => 0.0,
        }
    }

    /// Last frame's raster time.
    pub fn last_raster(&self) -> Duration {
        self.last_raster
    }

    /// Last frame's present time.
    pub fn last_present(&self) -> Duration {
        self.last_present
    }
}

/// Factories supplied by the caller (concrete backend crates), one per slot.
pub type MultiBackendFactories = [Option<BackendFactory>; 5];

/// Builder for a multi-backend demo window set.
pub struct MultiBackendConfig {
    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fullscreen: bool,
    pub(crate) fonts: Vec<Vec<u8>>,
    pub(crate) background: Color,
    pub(crate) initial: RenderBackend,
    pub(crate) show_hud: bool,
    pub(crate) fps_limit: FpsLimit,
    pub(crate) extensions: Vec<Box<dyn AppExtension>>,
    pub(crate) runtime: Option<Rc<RefCell<ReactiveRuntime>>>,
    pub(crate) factories: MultiBackendFactories,
}

impl MultiBackendConfig {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
    /// Borderless fullscreen on the current monitor (gallery launcher).
    pub fn fullscreen(mut self, on: bool) -> Self {
        self.fullscreen = on;
        self
    }
    pub fn font(mut self, _name: &str, bytes: &[u8]) -> Self {
        self.fonts.push(bytes.to_vec());
        self
    }
    pub fn background(mut self, color: impl Into<Color>) -> Self {
        self.background = color.into();
        self
    }
    pub fn initial(mut self, backend: RenderBackend) -> Self {
        self.initial = backend;
        self
    }
    pub fn show_hud(mut self, on: bool) -> Self {
        self.show_hud = on;
        self
    }
    pub fn fps_limit(mut self, limit: impl Into<FpsLimit>) -> Self {
        self.fps_limit = limit.into();
        self
    }
    pub fn extension(mut self, ext: impl AppExtension + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }
    pub fn runtime(mut self, runtime: Rc<RefCell<ReactiveRuntime>>) -> Self {
        self.runtime = Some(runtime);
        self
    }
    pub fn factories(mut self, factories: MultiBackendFactories) -> Self {
        self.factories = factories;
        self
    }
    pub fn run<E: IntoElement, F: Fn() -> E + 'static>(self, root_fn: F) {
        run_multi(self, root_fn);
    }
}

/// Entrypoint for multi-backend demos: one shared state, N surfaces.
pub struct MultiBackend;

impl MultiBackend {
    pub fn config(factories: MultiBackendFactories) -> MultiBackendConfig {
        MultiBackendConfig {
            title: "Aurora Multi-Backend".into(),
            width: 800,
            height: 600,
            fullscreen: false,
            fonts: Vec::new(),
            background: Color::WHITE,
            initial: RenderBackend::TinySkia,
            show_hud: true,
            fps_limit: FpsLimit::Auto,
            extensions: Vec::new(),
            runtime: None,
            factories,
        }
    }
}

struct LiveSurface {
    window: Arc<Window>,
    backend: Box<dyn Backend>,
}

/// Damage-by-age resolver backed by the shared engine's frame history.
struct EngineDamage<'e> {
    engine: &'e Engine,
}

impl<'e> EngineDamage<'e> {
    fn new(engine: &'e Engine) -> Self {
        Self { engine }
    }
}

impl DamageResolver for EngineDamage<'_> {
    #[inline]
    fn for_age(&mut self, age: u8) -> DamageRegion {
        self.engine.damage_for_age(age)
    }
}

/// Scanline post-process adapter: extensions first, then the debugger HUD pass.
struct CombinedScanline<'e> {
    extensions: &'e mut [Box<dyn AppExtension>],
    debugger: &'e mut VisualDebugger,
}

impl FramePostProcess for CombinedScanline<'_> {
    fn apply(&mut self, buffer: &mut [u32], damage: &DamageRegion, stride: usize, height: usize) {
        for ext in self.extensions.iter_mut() {
            ext.on_present(buffer, damage, stride, height);
        }
        self.debugger.on_present(buffer, damage, stride, height);
    }
}

fn ensure_surface(
    slot: usize,
    name: &str,
    title: &str,
    win_w: u32,
    win_h: u32,
    fullscreen: bool,
    event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
    factory: Option<BackendFactory>,
    text_ctx: &crate::text::TextContext,
    surfaces: &mut HashMap<usize, LiveSurface>,
    failed: &mut HashMap<usize, String>,
) -> bool {
    if surfaces.contains_key(&slot) || failed.contains_key(&slot) {
        return surfaces.contains_key(&slot);
    }
    let Some(factory) = factory else {
        failed.insert(slot, "no factory supplied".into());
        return false;
    };
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(format!("{title} — {name}"))
            .with_inner_size(winit::dpi::LogicalSize::new(win_w, win_h))
            .with_fullscreen(fullscreen.then(|| winit::window::Fullscreen::Borderless(None)))
            .with_resizable(true)
            .with_position(winit::dpi::LogicalPosition::new(
                80.0 + slot as f64 * 60.0,
                80.0 + slot as f64 * 40.0,
            ))
            .build(event_loop)
            .expect("window init failed"),
    );
    let physical = window.inner_size();
    match factory(
        Arc::clone(&window),
        physical.width.max(1),
        physical.height.max(1),
        text_ctx.clone(),
    ) {
        Ok(backend) => {
            tracing::info!(slot, backend = name, "backend live");
            surfaces.insert(slot, LiveSurface { window, backend });
            true
        }
        Err(e) => {
            tracing::error!(slot, error = ?e, "backend init failed");
            failed.insert(slot, format!("{e:?}"));
            false
        }
    }
}

fn run_multi<E: IntoElement, F: Fn() -> E + 'static>(config: MultiBackendConfig, root_fn: F) {
    let event_loop = EventLoopBuilder::with_user_event()
        .build()
        .unwrap_or_else(crate::app::runner::fail_display_init);
    let proxy = event_loop.create_proxy();
    let runtime = config
        .runtime
        .unwrap_or_else(|| Rc::new(RefCell::new(ReactiveRuntime::new())));
    let root_factory: SharedFactory = Rc::new(move || root_fn().into_element());
    let cx = Cx::new(Rc::clone(&runtime));
    runtime.borrow_mut().begin_build_window();
    let root = cx.enter_keyed("root", || root_factory()).into_element();

    let mut single = BACKEND_ORDER
        .iter()
        .position(|b| *b == config.initial)
        .unwrap_or(0);

    let mut debugger = VisualDebugger::new();
    debugger.show_hud = config.show_hud;

    let mut surfaces: HashMap<usize, LiveSurface> = HashMap::new();
    let mut stats: HashMap<usize, BackendStats> = HashMap::new();
    let mut failed: HashMap<usize, String> = HashMap::new();

    // The engine is always demo-sized; windows track it 1:1.
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, config.width, config.height);
    engine.set_background(config.background);
    engine.set_root_factory(Rc::clone(&root_factory));
    runtime.borrow_mut().end_build_window(true);
    for f in &config.fonts {
        engine.load_font(f);
    }
    engine.runtime().borrow_mut().set_on_dirty(move || {
        let _ = proxy.send_event(());
    });
    let text_ctx = engine.text_context();

    ensure_surface(
        single,
        backend_display_name(BACKEND_ORDER[single]),
        &config.title,
        config.width,
        config.height,
        config.fullscreen,
        &event_loop,
        config.factories[single],
        &text_ctx,
        &mut surfaces,
        &mut failed,
    );
    if surfaces.is_empty() {
        panic!("no backend could be created: {failed:?}");
    }
    for s in surfaces.values() {
        s.window.request_redraw();
    }

    let mut pending_resize: HashMap<usize, (u32, u32)> = HashMap::new();
    let mut last_present = Instant::now();
    let mut pointer = PointerBridge::new();
    let mut cursor = CursorIcon::Default;
    let mut os_clipboard = OsClipboardSync::new();
    let mut last_stats_log = Instant::now();
    // Fresh window canvases hold undefined pixels: the first frame presented
    // to a different surface than the previous one repaints fully, or only
    // incremental damage reaches the new canvas (black tiles).
    let mut last_presented_slot: Option<usize> = None;
    let mut extensions = config.extensions;
    let factories = config.factories;
    let title = config.title.clone();
    let (cfg_w, cfg_h) = (config.width, config.height);
    let fullscreen = config.fullscreen;
    let _fps_limit = config.fps_limit;
    // Last idle extension lap: at rest no frames are drawn, so without this
    // `DevLoop::poll` (hot-reload watch → build → refresh) never runs and
    // source edits go unnoticed. Throttled — the lap itself walks mtimes.
    let mut last_idle_poll = Instant::now();

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } => {
                let slot = surfaces
                    .iter()
                    .find(|(_, s)| s.window.id() == window_id)
                    .map(|(k, _)| *k);
                let Some(slot) = slot else { return };
                if let WindowEvent::KeyboardInput { event: key, .. } = &event {
                    if key.state == ElementState::Pressed
                        && key.logical_key == Key::Named(NamedKey::Escape)
                    {
                        elwt.exit();
                        return;
                    }
                    let switch = key.state == ElementState::Pressed
                        && handle_switch_key(&key.logical_key, single).is_some();
                    if switch {
                        let next = handle_switch_key(&key.logical_key, single).unwrap();
                        single = next;
                        let tc = engine.text_context();
                        ensure_surface(
                            single,
                            backend_display_name(BACKEND_ORDER[single]),
                            &title,
                            cfg_w,
                            cfg_h,
                            fullscreen,
                            elwt,
                            factories[single],
                            &tc,
                            &mut surfaces,
                            &mut failed,
                        );
                        if let Some(s) = surfaces.get(&single) {
                            s.window.request_redraw();
                            s.window.focus_window();
                        }
                        tracing::info!(
                            engine = backend_display_name(BACKEND_ORDER[single]),
                            "switch: single"
                        );
                        return;
                    }
                }
                let mut need_redraw = false;
                need_redraw |= pointer.dispatch(&event, &mut engine);
                let icon = engine.cursor_icon();
                if icon != cursor {
                    cursor = icon;
                    if let Some(s) = surfaces.get(&slot) {
                        s.window.set_cursor_icon(match icon {
                            CursorIcon::Default => winit::window::CursorIcon::Default,
                            CursorIcon::Text => winit::window::CursorIcon::Text,
                        });
                    }
                }
                need_redraw |= debugger.on_event(&event);
                // The loop owns one debugger (above); a config-registered
                // debugger would double-handle every edge-triggered key
                // (B toggles twice = no-op, '.' steps twice), so only
                // non-debugger extensions run here. Level-state keys
                // (F8, F9-F12, brackets) converge either way, which is why
                // this went unnoticed until B.
                for ext in extensions.iter_mut() {
                    let is_debugger = ext.as_debugger().is_some();
                    if !is_debugger && ext.on_event(&event) {
                        need_redraw = true;
                    }
                }
                match event {
                    WindowEvent::CloseRequested => {
                        surfaces.remove(&slot);
                        if surfaces.is_empty() {
                            elwt.exit();
                        }
                    }
                    WindowEvent::Resized(size) => {
                        pending_resize.insert(slot, (size.width.max(1), size.height.max(1)));
                        if let Some(s) = surfaces.get(&slot) {
                            s.window.request_redraw();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        // Frame the shared engine ONCE, then present the
                        // scene to the requesting surface.
                        if last_presented_slot != Some(slot) {
                            engine.request_full_repaint();
                            last_presented_slot = Some(slot);
                        }
                        let mut wants_redraw = need_redraw;
                        for ext in extensions.iter_mut() {
                            wants_redraw |= ext.on_update(&mut engine);
                        }
                        wants_redraw |= debugger.on_update(&mut engine);
                        let diag_full = engine.frame().diagnostics;
                        debugger.on_frame(&diag_full);
                        for ext in extensions.iter_mut() {
                            ext.on_frame(&diag_full);
                        }
                        let text_ctx = engine.text_context();
                        {
                            let resize = pending_resize.remove(&slot);
                            let Some(surf) = surfaces.get_mut(&slot) else {
                                return;
                            };
                            // Windows track the engine 1:1.
                            let (w, h) = resize.unwrap_or_else(|| engine.logical_size());
                            if (w, h) != engine.logical_size() {
                                surf.backend.resize(w, h);
                                engine.resize(w, h);
                            }
                            let (lw, lh) = engine.logical_size();
                            let caps = OverlayCaps {
                                backend: surf.backend.tag(),
                                scanline_overlays: surf.backend.supports_scanline(),
                                viewport: ResolvedRect::new(0.0, 0.0, lw as f32, lh as f32),
                                background: engine.background(),
                            };
                            for ext in extensions.iter_mut() {
                                ext.render_overlay(
                                    &mut OverlayPainter::new(engine.scene_mut()),
                                    &text_ctx,
                                    &caps,
                                    &diag_full,
                                );
                            }
                            debugger.render_overlay(
                                &mut OverlayPainter::new(engine.scene_mut()),
                                &text_ctx,
                                &caps,
                                &diag_full,
                            );
                            let extra_damage: SmallVec<[ResolvedRect; 4]> = extensions
                                .iter()
                                .filter_map(|ext| ext.overlay_damage())
                                .chain(debugger.overlay_damage())
                                .collect();
                            let damage = engine.current_damage().clone();
                            let report = {
                                let mut resolver = EngineDamage::new(&engine);
                                let mut scanline =
                                    caps.scanline_overlays.then(|| CombinedScanline {
                                        extensions: &mut extensions,
                                        debugger: &mut debugger,
                                    });
                                // Reborrow disjoint fields through raw pointers:
                                // `extensions`/`debugger` are not part of `surfaces`.
                                let frame = Present {
                                    scene: engine.scene(),
                                    damage: &damage,
                                    extra_damage: &extra_damage,
                                    surface: SurfaceRequest {
                                        background: engine.background(),
                                        width: w,
                                        height: h,
                                    },
                                    damage_resolver: &mut resolver,
                                    post_process: scanline
                                        .as_mut()
                                        .map(|pass| pass as &mut dyn FramePostProcess),
                                };
                                surf.backend.present(frame)
                            };
                            match report {
                                Ok(r) => {
                                    // Stash per-slot stats for the 1/s perf log below.
                                    stats
                                        .entry(slot)
                                        .or_insert_with(BackendStats::new)
                                        .record(r.raster, r.present);
                                }
                                Err(e) => {
                                    tracing::error!(slot, %e, "frame presentation failed");
                                }
                            }
                        }
                        last_present = Instant::now();
                        os_clipboard.sync(&engine);
                        if wants_redraw || engine.has_active_motion() {
                            if let Some(s) = surfaces.get(&slot) {
                                s.window.request_redraw();
                            }
                        }
                        if last_stats_log.elapsed() >= Duration::from_secs(1) {
                            last_stats_log = Instant::now();
                            if let Some(st) = stats.get(&slot) {
                                tracing::info!(
                                    engine = backend_display_name(BACKEND_ORDER[slot]),
                                    fps = st.avg_fps_5s(),
                                    raster_ms = st.last_raster().as_secs_f32() * 1000.0,
                                    present_ms = st.last_present().as_secs_f32() * 1000.0,
                                    "perf"
                                );
                            }
                        }
                    }
                    _ => {
                        if need_redraw {
                            if let Some(s) = surfaces.get(&slot) {
                                s.window.request_redraw();
                            }
                        }
                    }
                }
            }
            Event::UserEvent(()) => {
                if let Some(s) = surfaces.get(&single) {
                    s.window.request_redraw();
                }
            }
            Event::AboutToWait => {
                // Idle extension lap (hot-reload lives here): request a frame
                // when an extension has work, then ALWAYS schedule the next
                // wake. winit's default `ControlFlow::Wait` suspends the thread
                // until an OS event arrives — without an explicit `WaitUntil`
                // an idle window sleeps through source edits and only reloads
                // when the mouse (CursorMoved events) wakes it.
                if last_idle_poll.elapsed() >= Duration::from_millis(500) {
                    last_idle_poll = Instant::now();
                    if extensions.iter_mut().any(|ext| ext.on_idle()) {
                        if let Some(s) = surfaces.get(&single) {
                            s.window.request_redraw();
                        }
                    }
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(500),
                ));
                let _ = last_present;
            }
            _ => {}
        })
        .expect("Event loop execution failed");
}

/// Engine switch keys. Returns the newly focused slot when handled.
///
/// `1-5` select directly; Tab and F7 cycle round-robin.
fn handle_switch_key(key: &Key, single: BackendSlot) -> Option<BackendSlot> {
    match key {
        Key::Character(c) => match c.as_str() {
            "1" | "2" | "3" | "4" | "5" => {
                let idx = c.chars().next()?.to_digit(10)? as usize - 1;
                (idx < BACKEND_ORDER.len()).then_some(idx)
            }
            _ => None,
        },
        Key::Named(NamedKey::Tab) | Key::Named(NamedKey::F7) => Some(cycle_slot(single)),
        _ => None,
    }
}
