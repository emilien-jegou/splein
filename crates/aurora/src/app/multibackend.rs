// Single responsibility: Multi-backend demo runner — one shared Engine presented
// through up to four raster backends (switch + combine modes).
//
// One `Engine` (one reactive runtime, one `Scene`) frames once; each live
// backend presents that same scene into its own OS window. Backend switching
// reuses cached backends (lazy init, fail-soft per backend). Combine mode
// presents one frame to all backends for side-by-side perf comparison.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smallvec::SmallVec;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::EventLoopBuilder;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::app::runner::backend::{Backend, BackendFactory, DamageResolver, FramePostProcess, Present, SurfaceRequest};
use crate::app::runner::pointer::PointerBridge;
use crate::app::runner::{clipboard::OsClipboardSync, FpsLimit};
use crate::app::VisualDebugger;
use crate::dsl::component::IntoElement;
use crate::foundation::{Color, DamageRegion, ResolvedRect};
use crate::input::CursorIcon;
use crate::reactive::{Cx, ReactiveRuntime};
use crate::render::RenderBackend;
use crate::runtime::Engine;

/// Ordered backend slots: index selects, combine runs all live ones.
pub const BACKEND_ORDER: [RenderBackend; 4] = [
    RenderBackend::TinySkia,
    RenderBackend::Vello,
    RenderBackend::VelloCpu,
    RenderBackend::Wgpu,
];

/// Descriptive display name per backend slot.
pub fn backend_display_name(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::TinySkia => "tiny-skia",
        RenderBackend::Vello => "vello-gpu",
        RenderBackend::VelloCpu => "vello-cpu",
        RenderBackend::Wgpu => "wgpu",
    }
}

/// Which surfaces are live: one focused backend, or all of them at once.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    Single(usize),
    Combine,
}

impl DisplayMode {
    fn cycle_next(self) -> Self {
        match self {
            Self::Single(i) => Self::Single((i + 1) % BACKEND_ORDER.len()),
            Self::Combine => Self::Single(0),
        }
    }
}

/// Trailing-5s per-backend present stats for combine-mode comparison.
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
}

/// Factories supplied by the caller (concrete backend crates), one per slot.
pub type MultiBackendFactories = [Option<BackendFactory>; 4];

/// Builder for a multi-backend demo window set.
pub struct MultiBackendConfig {
    title: String,
    width: u32,
    height: u32,
    fullscreen: bool,
    fonts: Vec<Vec<u8>>,
    background: Color,
    initial: RenderBackend,
    combine_on_boot: bool,
    show_hud: bool,
    fps_limit: FpsLimit,
    extensions: Vec<Box<dyn AppExtension>>,
    runtime: Option<Rc<RefCell<ReactiveRuntime>>>,
    factories: MultiBackendFactories,
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
    pub fn combine(mut self, on: bool) -> Self {
        self.combine_on_boot = on;
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
            combine_on_boot: false,
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
    slot: usize,
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
    title: &str,
    width: u32,
    height: u32,
    fullscreen: bool,
    event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
    factories: &MultiBackendFactories,
    text_ctx: &crate::text::TextContext,
    surfaces: &mut HashMap<usize, LiveSurface>,
    failed: &mut HashMap<usize, String>,
) -> bool {
    if surfaces.contains_key(&slot) || failed.contains_key(&slot) {
        return surfaces.contains_key(&slot);
    }
    let Some(factory) = factories[slot] else {
        failed.insert(slot, "no factory supplied".into());
        return false;
    };
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(format!(
                "{title} — {}",
                backend_display_name(BACKEND_ORDER[slot])
            ))
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_fullscreen(
                fullscreen.then(|| winit::window::Fullscreen::Borderless(None)),
            )
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
            tracing::info!(slot, backend = backend_display_name(BACKEND_ORDER[slot]), "backend live");
            surfaces.insert(slot, LiveSurface { window, backend, slot });
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
        .expect("Display init failed");
    let proxy = event_loop.create_proxy();
    let runtime = config
        .runtime
        .unwrap_or_else(|| Rc::new(RefCell::new(ReactiveRuntime::new())));
    let cx = Cx::new(Rc::clone(&runtime));
    let root = cx.enter_keyed("root", root_fn).into_element();

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, config.width, config.height);
    engine.set_background(config.background);
    for f in &config.fonts {
        engine.load_font(f);
    }
    engine.runtime().borrow_mut().set_on_dirty(move || {
        let _ = proxy.send_event(());
    });

    let initial_slot = BACKEND_ORDER
        .iter()
        .position(|b| *b == config.initial)
        .unwrap_or(0);
    let mut mode = if config.combine_on_boot {
        DisplayMode::Combine
    } else {
        DisplayMode::Single(initial_slot)
    };

    let mut debugger = VisualDebugger::new();
    debugger.show_hud = config.show_hud;

    let mut surfaces: HashMap<usize, LiveSurface> = HashMap::new();
    let mut stats: HashMap<usize, BackendStats> = HashMap::new();
    let mut failed: HashMap<usize, String> = HashMap::new();
    let text_ctx = engine.text_context();

    match mode {
        DisplayMode::Single(i) => {
            ensure_surface(i, &config.title, config.width, config.height, config.fullscreen, &event_loop, &config.factories, &text_ctx, &mut surfaces, &mut failed);
        }
        DisplayMode::Combine => {
            for slot in 0..BACKEND_ORDER.len() {
                ensure_surface(slot, &config.title, config.width, config.height, config.fullscreen, &event_loop, &config.factories, &text_ctx, &mut surfaces, &mut failed);
            }
        }
    }
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
    let mut extensions = config.extensions;
    let factories = config.factories;
    let title = config.title.clone();
    let (cfg_w, cfg_h) = (config.width, config.height);
    let fullscreen = config.fullscreen;
    let _fps_limit = config.fps_limit;

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
                        && handle_switch_key(&key.logical_key, mode).is_some();
                    if switch {
                        let next = handle_switch_key(&key.logical_key, mode).unwrap();
                        mode = next;
                        match mode {
                            DisplayMode::Single(i) => {
                                let tc = engine.text_context();
                                ensure_surface(i, &title, cfg_w, cfg_h, fullscreen, elwt, &factories, &tc, &mut surfaces, &mut failed);
                                if let Some(s) = surfaces.get(&i) {
                                    s.window.request_redraw();
                                    s.window.focus_window();
                                }
                                tracing::info!(engine = backend_display_name(BACKEND_ORDER[i]), "switch: single");
                            }
                            DisplayMode::Combine => {
                                let tc = engine.text_context();
                                for s in 0..BACKEND_ORDER.len() {
                                    ensure_surface(s, &title, cfg_w, cfg_h, fullscreen, elwt, &factories, &tc, &mut surfaces, &mut failed);
                                }
                                for s in surfaces.values() {
                                    s.window.request_redraw();
                                }
                                tracing::info!("switch: combine (all live backends)");
                            }
                        }
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
                need_redraw |= extensions.iter_mut().any(|ext| ext.on_event(&event));
                match event {
                    WindowEvent::CloseRequested => {
                        surfaces.remove(&slot);
                        if surfaces.is_empty() {
                            elwt.exit();
                        }
                    }
                    WindowEvent::Resized(size) => {
                        pending_resize
                            .insert(slot, (size.width.max(1), size.height.max(1)));
                        if let Some(s) = surfaces.get(&slot) {
                            s.window.request_redraw();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        // Frame the shared engine ONCE, then present the same scene
                        // to the requesting surface (+ the rest in combine mode).
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
                        let mut order: Vec<usize> = vec![slot];
                        if mode == DisplayMode::Combine {
                            let mut rest: Vec<usize> = surfaces
                                .keys()
                                .filter(|k| **k != slot)
                                .copied()
                                .collect();
                            rest.sort_unstable();
                            order.extend(rest);
                        }
                        for s in order {
                            let resize = pending_resize.remove(&s);
                            let Some(surf) = surfaces.get_mut(&s) else { continue };
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
                                let mut scanline = caps.scanline_overlays.then(|| {
                                    CombinedScanline {
                                        extensions: &mut extensions,
                                        debugger: &mut debugger,
                                    }
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
                                    stats
                                        .entry(s)
                                        .or_insert_with(BackendStats::new)
                                        .record(r.raster, r.present);
                                }
                                Err(e) => {
                                    tracing::error!(slot = s, %e, "frame presentation failed");
                                }
                            }
                        }
                        last_present = Instant::now();
                        os_clipboard.sync(&engine);
                        if wants_redraw || engine.has_active_motion() {
                            match mode {
                                DisplayMode::Single(_) => {
                                    if let Some(s) = surfaces.get(&slot) {
                                        s.window.request_redraw();
                                    }
                                }
                                DisplayMode::Combine => {
                                    for s in surfaces.values() {
                                        s.window.request_redraw();
                                    }
                                }
                            }
                        }
                        if mode == DisplayMode::Combine
                            && last_stats_log.elapsed() >= Duration::from_secs(1)
                        {
                            last_stats_log = Instant::now();
                            let mut line = String::from("perf |");
                            for i in 0..BACKEND_ORDER.len() {
                                match stats.get(&i) {
                                    Some(st) => line.push_str(&format!(
                                        " {} {:.0}fps r{:.1}ms p{:.1}ms |",
                                        backend_display_name(BACKEND_ORDER[i]),
                                        st.avg_fps_5s(),
                                        st.last_raster.as_secs_f32() * 1000.0,
                                        st.last_present.as_secs_f32() * 1000.0
                                    )),
                                    None => line.push_str(&format!(
                                        " {} -- |",
                                        backend_display_name(BACKEND_ORDER[i])
                                    )),
                                }
                            }
                            tracing::info!("{line}");
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
                match mode {
                    DisplayMode::Single(i) => {
                        if let Some(s) = surfaces.get(&i) {
                            s.window.request_redraw();
                        }
                    }
                    DisplayMode::Combine => {
                        for s in surfaces.values() {
                            s.window.request_redraw();
                        }
                    }
                }
            }
            Event::AboutToWait => {
                let _ = last_present;
            }
            _ => {}
        })
        .expect("Event loop execution failed");
}

/// Switch/combine keys. Returns the new mode when handled.
fn handle_switch_key(key: &Key, mode: DisplayMode) -> Option<DisplayMode> {
    match key {
        Key::Character(c) => match c.as_str() {
            "1" | "2" | "3" | "4" => {
                let idx = c.chars().next()?.to_digit(10)? as usize - 1;
                Some(DisplayMode::Single(idx))
            }
            "c" | "C" => Some(match mode {
                DisplayMode::Combine => DisplayMode::Single(0),
                DisplayMode::Single(_) => DisplayMode::Combine,
            }),
            _ => None,
        },
        Key::Named(NamedKey::Tab) => Some(match mode {
            DisplayMode::Single(_) => mode.cycle_next(),
            DisplayMode::Combine => DisplayMode::Single(0),
        }),
        _ => None,
    }
}
