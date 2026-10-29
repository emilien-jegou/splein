// Single responsibility: engine-level overlay host runner.
//
// This is the whole layer-shell host: Wayland connection, layer surface,
// hot-reload dev loop. Apps configure it with [`OverlayConfig`] and call
// [`overlay_run`] — their `main.rs` stays a ~50-line config (the same
// shape as `devkit_run` owning the winit hot loop in riode-shared).
//
// (surface-local coordinates, no mismatch possible) or, where the
// compositor needs it, one overlay layer surface per island
// ([`crate::island_layers`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::{
    compositor::CompositorHandler, delegate_compositor, delegate_layer, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm, output::OutputHandler,
    registry::ProvidesRegistryState, registry_handlers,
    seat::pointer::{PointerEvent as SctkPointerEvent, PointerEventKind, PointerHandler},
    seat::{Capability, SeatHandler},
    shell::wlr_layer::LayerShellHandler,
    shm::ShmHandler,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_region, wl_seat, wl_subsurface, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use riode_shared::dsl::IntoElement;
use riode_shared::foundation::{Color, Point, Radius, ResolvedRect};
use riode_shared::hot_reload::{
    blocking_hot_build, component_abi_fingerprint, host_sentinel_address, ComponentRootFn,
    ComponentVersionFn, DevLoop, DevLoopConfig, DylibReloader, HotWindowFn, HotWindowInfo,
    SharedFactory,
};
use riode_shared::input::{PointerButton, PointerEvent, PointerPhase};
use riode_shared::internal::{group, text};
use riode_shared::reactive::{Cx, ReactiveRuntime};
use riode_shared::runtime::Engine;
use riode_shared::scene::extract_islands;
use riode_shared::tiny_skia::HeadlessRaster;

use crate::island_layers::IslandLayerPool;
use crate::item_hints::{RiodeItemManagerV1, RiodeItemV1};
use crate::{
};
use crate::surface::create_overlay_surface;

/// How the overlay surface covers the output.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SurfacePolicy {
    Fullscreen,
    /// Fixed-size bottom-anchored layer (dock shape); the size comes from
    /// the hot window chrome at boot and layer-shell configure after.
    BottomFixed,
}

/// island — no preset names anywhere.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
    /// surface-local coordinates (one surface, no coordinate mismatch).
    ParentSurface,
    /// One overlay layer surface per island (see [`crate::island_layers`]).
    /// Kept for hosts whose compositor path needs it.
    IslandLayers,
}

/// Where the hot UI comes from: the generative example crate, or the
/// app's own cdylib target (standalone — sources live next to the app,
/// no `examples/` dir needed).
pub struct HotConfig {
    /// Cargo package whose cdylib holds the UI (rebuilt by the loop).
    /// Generative: `"riode-hot-gen-ui"`. Standalone: the app package
    /// itself (bin + `[lib] crate-type = ["cdylib"]`).
    pub package: &'static str,
    /// `RIODE_HOT_EXAMPLE` value for the generative crate (`None` keeps
    /// the legacy per-package behavior: the loop builds + watches the
    /// package itself — the standalone path).
    pub example: Option<&'static str>,
    /// Watch dir, workspace-root-relative (`None` = DevLoop default:
    /// `examples/<package>/`, falling back to crate dirs). Standalone
    /// apps point at their own sources, e.g. `"apps/dock/src"`.
    pub watch_dir: Option<&'static str>,
    /// `scripts/hot-sets/<name>` entry for `companion_set` (must equal the
    /// hand-build set or rival `riode` units break dylib loads).
    pub companion_set: &'static str,
}

/// Everything an overlay app configures. Everything else — connection,
/// runner's job.
pub struct OverlayConfig {
    pub namespace: &'static str,
    pub hot: HotConfig,
    pub surface: SurfacePolicy,
    /// Off = click-through overlay that never takes pointer.
    pub interactive: bool,
    /// Boot font, e.g. `include_bytes!(".../inter-var.ttf")`.
    pub font_bytes: &'static [u8],
}

fn placeholder_root() -> impl IntoElement {
    group()
        .width(400.0)
        .height(100.0)
        .children([text("building hot-ui…").size(16.0)])
        .into_element()
}

// Empty bodies: these interfaces genuinely have no events.
delegate_noop!(OverlayHost: wl_region::WlRegion);
delegate_noop!(OverlayHost: wl_subsurface::WlSubsurface);
delegate_noop!(OverlayHost: RiodeItemV1);

// NOT delegate_noop: the manager sends `finished` and noop traps any
// received event. Ignored — the global simply goes away with the plugin.
impl Dispatch<RiodeItemManagerV1, ()> for OverlayHost {
    fn event(
        _: &mut Self,
        _: &RiodeItemManagerV1,
        _: <RiodeItemManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(OverlayHost: ExtBackgroundEffectSurfaceV1);

// NOT delegate_noop: the compositor sends `capabilities` on the MANAGER
// object and noop traps any received event with unreachable!(). Ignored.
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for OverlayHost {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        _: <ExtBackgroundEffectManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Maps the shared runtime globally FIRST, so the hot UI image (dlopened
/// later) resolves riode to this one live image instead of embedding its
/// own copy. Leaked on purpose: unmapping a runtime out from under live
/// hook scopes segfaults on exit; the OS reclaims it.
fn preload_shared(path: &std::path::Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    unsafe {
        match libloading::os::unix::Library::open(
            Some(std::ffi::OsStr::new(path.as_ref())),
            libc::RTLD_NOW | libc::RTLD_GLOBAL,
        ) {
            Ok(lib) => {
                std::mem::forget(lib);
                Ok(())
            }
            Err(e) => Err(format!("cannot preload shared runtime {path}: {e}")),
        }
    }
}

/// transient nodes come and go, the main island stays the biggest rect.
    islands
        .iter()
        .max_by(|a, b| {
            let area = |i: &&riode_shared::scene::Island| i.rect.width * i.rect.height;
            area(a).partial_cmp(&area(b)).unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// (emit-stamped with the node's declared corners): `Radius::Max` becomes
/// a stadium (half the short edge), matching the engine's SDF.
fn shape_radii(island: &riode_shared::scene::Island, w: f32, h: f32) -> [f64; 4] {
    match island.radius() {
        Radius::ZERO => [0.0; 4],
        Radius::Scalar(v) => [v as f64; 4],
        Radius::Max => [w.min(h) as f64 / 2.0; 4],
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => [
            top_left as f64,
            top_right as f64,
            bottom_right as f64,
            bottom_left as f64,
        ],
    }
}

pub struct OverlayHost {
    qh: QueueHandle<Self>,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    #[allow(dead_code)]
    compositor_state: CompositorState,
    #[allow(dead_code)]
    layer_shell: LayerShell,
    shm_state: Shm,
    output: Option<wl_output::WlOutput>,
    /// Output device scale for the compositor wire pack (pack space must
    /// equal the plugin's device-px capture space). Recorded from
    /// `scale_factor_changed` on our surface; islands share the output.
    output_scale: i32,
    surface: Option<HostSurface>,
    engine: Engine,
    raster: HeadlessRaster,
    cfg: OverlayHostCfg,
    // parent layer surface (the protocols allow at most one per surface).
    effect: Option<ExtBackgroundEffectSurfaceV1>,
    item: Option<RiodeItemV1>,
    // the managers the pool sync mints per-island effects/items from
    // (cloned per frame; proxies are cheap handles).
    islands: IslandLayerPool,
    island_bg: Option<ExtBackgroundEffectManagerV1>,
    island_item: Option<RiodeItemManagerV1>,
    input_rect: Option<(i32, i32, i32, i32)>,
    last_frame: Option<Instant>,
    last_island_log: Instant,
    pointer_down: bool,
    dev: DevLoop,
    shared_root: Rc<Cell<Option<ComponentRootFn>>>,
    factory_installed: bool,
    pending_activation: bool,
    /// An island configure landed while the parent surface was frame-locked
    /// (`paint` early-returns under `waiting_for_frame`, and its uploads for
    /// configured islands ride along): repaint on the next frame callback.
    /// A static view never repaints otherwise — islands stay configured but
    pending_island_paint: bool,
}

/// Runner-owned copy of the app-facing config (non-`'static` bits owned).
struct OverlayHostCfg {
    namespace: String,
    interactive: bool,
    island_layers: bool,
    bottom_fixed: bool,
}

struct HostSurface {
    layer: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    configured: bool,
    waiting_for_frame: bool,
}

impl OverlayHost {
    fn tick(&mut self, dt: f32) {
        self.engine.advance(dt);
        self.engine.frame();
    }

    /// Hot-loop lap: watch → background cargo build → fresh dylib observed →
    /// resolve entry points and swap the root factory. Returns true when
    /// the engine should present another frame.
    fn hot_poll(&mut self) -> bool {
        let shared = Rc::clone(&self.shared_root);
        let installed = &mut self.factory_installed;
        let namespace = self.cfg.namespace.clone();
        self.dev.poll_update(
            &mut self.engine,
            &mut self.pending_activation,
            &mut |dev, engine| Self::activate(dev, engine, &shared, installed, &namespace),
            &mut || false,
        )
    }

    fn activate(
        dev: &mut DevLoop,
        engine: &mut Engine,
        shared_root: &Rc<Cell<Option<ComponentRootFn>>>,
        factory_installed: &mut bool,
        namespace: &str,
    ) -> bool {
        // Single-instance guard: a UI image that resolved riode to a
        // different runtime copy would panic its component bodies on an
        // empty hook stack. Refuse and keep the previous UI instead.
        match dev.loader().bound_address(b"riode_runtime_sentinel") {
            Ok(addr) if addr == host_sentinel_address() => {}
            Ok(_) => {
                tracing::error!("{namespace}: hot-ui bound riode to a different runtime instance (build with -C prefer-dynamic); keeping previous UI");
                return true;
            }
            Err(e) => {
                tracing::warn!("{namespace}: hot-ui exports no runtime sentinel ({e})");
                return true;
            }
        }
        let root: ComponentRootFn = unsafe {
            let version: libloading::Symbol<ComponentVersionFn> =
                match dev.loader().get(b"riode_hot_version") {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!("{namespace}: hot-ui missing riode_hot_version ({e})");
                        return true;
                    }
                };
            if version() != component_abi_fingerprint() {
                tracing::warn!("{namespace}: hot-ui ABI fingerprint mismatch; keeping previous UI");
                return true;
            }
            match dev.loader().get::<ComponentRootFn>(b"riode_component_root") {
                Ok(f) => *f,
                Err(e) => {
                    tracing::warn!("{namespace}: hot-ui missing riode_component_root ({e})");
                    return true;
                }
            }
        };
        tracing::info!("{namespace}: hot-ui swapped ({:?} retired images)", dev.loader().retired_count());
        shared_root.set(Some(root));
        if !*factory_installed {
            *factory_installed = true;
            let shared = Rc::clone(shared_root);
            let factory: SharedFactory = Rc::new(move || {
                let Some(root) = shared.get() else {
                    return placeholder_root().into_element();
                };
                root()
            });
            engine.set_root_factory(factory);
        }
        engine.queue_full_reload();
        true
    }

    /// clicks outside fall through to the windows below. Interactive
    /// hosts only; takes effect on the next commit, which paint()
    /// performs immediately after.
    fn sync_input_region(&mut self, islands: &[riode_shared::scene::Island]) {
        let r = &bar.rect;
        let rect = (
            r.x.floor() as i32,
            r.y.floor() as i32,
            (r.right().ceil() as i32 - r.x.floor() as i32).max(1),
            (r.bottom().ceil() as i32 - r.y.floor() as i32).max(1),
        );
        if self.input_rect == Some(rect) {
            return;
        }
        self.input_rect = Some(rect);
        let Some(surf) = self.surface.as_ref() else { return };
        let region = self.compositor_state.wl_compositor().create_region(&self.qh, ());
        region.add(rect.0, rect.1, rect.2, rect.3);
        surf.layer.wl_surface().set_input_region(Some(&region));
        region.destroy();
    }

    /// material + rounded-rect shape — all in surface-local coordinates.
        let r = &bar.rect;
        let w = (r.right().ceil() as i32 - r.x.floor() as i32).max(1) as f32;
        let h = (r.bottom().ceil() as i32 - r.y.floor() as i32).max(1) as f32;
        if let Some(effect) = self.effect.as_ref() {
                &self.qh,
                self.compositor_state.wl_compositor(),
                effect,
                &[ResolvedRect::new(r.x, r.y, w, h)],
            );
        }
        if let Some(item) = self.item.as_ref() {
            let radii = shape_radii(bar, w, h);
            // The bar is a single island: pack its material against its
            // own rect at the output scale, like every other island.
                    ResolvedRect::new(r.x, r.y, w, h),
                    g,
                    self.output_scale as f32,
                )
            });
            upload_effect_stages(item, bar.effects.as_ref());
            upload_item_shape(item, r.x as f64, r.y as f64, w as f64, h as f64, radii);
        }
    }

    fn paint(&mut self) {
        if self.surface.as_ref().is_none_or(|s| !s.configured || s.waiting_for_frame) {
            return;
        }
        let (w, h) = {
            let surf = self.surface.as_ref().unwrap();
            (surf.width, surf.height)
        };
        if w == 0 || h == 0 {
            return;
        }
        // Past every guard: this paint runs the island sync (uploads for
        // configured islands), so any pending island repaint is consumed.
        self.pending_island_paint = false;
        // Islands first: extracted from this frame's scene, in paint order.
        let islands = extract_islands(self.engine.scene());
        if self.last_island_log.elapsed().as_secs() >= 2 {
            self.last_island_log = Instant::now();
            let list: Vec<String> = islands
                .iter()
                .map(|i| {
                    format!(
                        "rect=({:.0},{:.0},{:.0}x{:.0}) {}",
                        i.rect.x,
                        i.rect.y,
                        i.rect.width,
                        i.rect.height,
                    )
                })
                .collect();
            tracing::info!("{}: islands({}): {}", self.cfg.namespace, islands.len(), list.join(" | "));
        }
        if self.cfg.interactive {
            self.sync_input_region(&islands);
        }
        if !self.cfg.island_layers {
        }
        let pixmap = self.raster.draw_engine(&mut self.engine);

        // Parent: full-scene buffer (robust fallback under the islands).
        let stride = (w * 4) as i32;
        let Some(surf) = self.surface.as_mut() else {
            return;
        };
        if !surf.configured || surf.waiting_for_frame {
            return;
        }
        let Ok((buffer, canvas)) = surf.pool.create_buffer(
            w as i32,
            h as i32,
            stride,
            wayland_client::protocol::wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        // tiny-skia Pixmaps are premultiplied RGBA bytes (R,G,B,A in memory);
        // wl_shm Argb8888 on little-endian is (B,G,R,A) — R and B must swap
        // or every hue renders wrong.
        for (d, s) in canvas
            .chunks_exact_mut(4)
            .zip(pixmap.data().chunks_exact(4))
        {
            let px = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
            let sw = (px & 0xFF00FF00) | ((px & 0x000000FF) << 16) | ((px & 0x00FF0000) >> 16);
            d.copy_from_slice(&sw.to_le_bytes());
        }

        if self.cfg.island_layers {
            // One overlay layer surface per island (subsurface regions are
            // dropped by core 0.56): each carries the island's pixels plus
            let qh = self.qh.clone();
            let bg = self.island_bg.clone();
            let item_mgr = self.island_item.clone();
            let output = self.output.clone();
            let namespace = self.cfg.namespace.clone();
            self.islands.sync(
                &qh,
                &self.layer_shell,
                &self.compositor_state,
                &self.shm_state,
                output.as_ref(),
                bg.as_ref(),
                item_mgr.as_ref(),
                self.output_scale as f32,
                &namespace,
                &islands,
                pixmap.data(),
                w,
                h,
            );
        }

        let wl_surf = surf.layer.wl_surface().clone();
        wl_surf.frame(&self.qh, wl_surf.clone());
        surf.waiting_for_frame = true;
        wl_surf.attach(Some(buffer.wl_buffer()), 0, 0);
        wl_surf.damage_buffer(0, 0, w as i32, h as i32);
        wl_surf.commit();
    }

    fn pointer_event(&mut self, position: Point, phase: PointerPhase) {
        if !self.cfg.interactive {
            return;
        }
        self.engine.queue_pointer_event(PointerEvent {
            position,
            button: PointerButton::Primary,
            phase,
            click_count: 0,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
        });
        self.tick(0.0);
        self.paint();
    }
}

delegate_compositor!(OverlayHost);
delegate_layer!(OverlayHost);
delegate_output!(OverlayHost);
delegate_shm!(OverlayHost);
delegate_seat!(OverlayHost);
delegate_pointer!(OverlayHost);
delegate_registry!(OverlayHost);

impl ProvidesRegistryState for OverlayHost {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl CompositorHandler for OverlayHost {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        // Our main surface carries the output scale islands pack
        // against (layer surfaces share the output in practice).
        let ours = self
            .surface
            .as_ref()
            .is_some_and(|s| s.layer.wl_surface() == surface);
        if ours {
            self.output_scale = scale.max(1);
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(surf) = self.surface.as_mut() {
            surf.waiting_for_frame = false;
        }
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0.0166, |prev| (now - prev).as_secs_f32().min(0.05));
        self.last_frame = Some(now);
        self.tick(dt);
        // Hot-loop lap runs every frame: edits rebuild in the background
        // and the overlay swaps without restarting.
        // Keep the callback chain alive only while something moves; at rest
        // the compositor holds the last frame and we sleep until input.
        let hot_redraw = self.hot_poll();
        if hot_redraw || self.pending_island_paint || self.engine.has_active_motion() {
            self.paint();
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for OverlayHost {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for OverlayHost {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl LayerShellHandler for OverlayHost {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        conf: LayerSurfaceConfigure,
        _: u32,
    ) {
        // Island layers carry client-driven sizes; any configure acks them.
        if self.cfg.island_layers {
            if let Some(island) = self.islands.find_mut(layer) {
                island.configured = true;
                // The parent may be frame-locked right now (waiting_for_frame);
                // if paint() early-returns, the frame() callback picks this up.
                self.pending_island_paint = true;
                self.tick(0.0);
                self.paint();
                return;
            }
        }
        let (mut w, mut h) = conf.new_size;
        // Bottom-fixed surfaces boot from the hot chrome; a zero configure
        // keeps it. Fullscreen surfaces always get a real size.
        if self.cfg.bottom_fixed {
            if w == 0 {
                w = self.surface.as_ref().map(|s| s.width).unwrap_or(900);
            }
            if h == 0 {
                h = self.surface.as_ref().map(|s| s.height).unwrap_or(220);
            }
        } else {
            w = w.max(1);
            h = h.max(1);
        }
        if let Some(surf) = self.surface.as_mut() {
            surf.width = w;
            surf.height = h;
            surf.configured = true;
            if self.engine.logical_size() != (w, h) {
                self.engine.resize(w, h);
            }
        }
        self.tick(0.0);
        self.paint();
    }
}

impl SeatHandler for OverlayHost {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Pointer {
            let _ = self.seat_state.get_pointer(qh, &seat);
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for OverlayHost {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        evs: &[SctkPointerEvent],
    ) {
        if !self.cfg.interactive {
            return;
        }
        for ev in evs {
            let at = Point::new(ev.position.0 as f32, ev.position.1 as f32);
            match ev.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    let phase = if self.pointer_down {
                        PointerPhase::Drag
                    } else {
                        PointerPhase::Move
                    };
                    self.pointer_event(at, phase);
                }
                PointerEventKind::Leave { .. } => {
                    // Off-surface: park the pointer outside content so hover
                    // states release.
                    self.pointer_event(Point::new(-100.0, -100.0), PointerPhase::Move);
                }
                PointerEventKind::Press { button: 0x110, .. } => {
                    self.pointer_down = true;
                    self.pointer_event(at, PointerPhase::Down);
                }
                PointerEventKind::Release { button: 0x110, .. } => {
                    self.pointer_down = false;
                    self.pointer_event(at, PointerPhase::Up);
                }
                _ => {}
            }
        }
    }
}

fn dev_loop_config(cfg: &OverlayConfig, root: std::path::PathBuf) -> DevLoopConfig {
    DevLoopConfig {
        package: cfg.hot.package.to_string(),
        example: cfg.hot.example.map(str::to_string),
        workspace_root: root.clone(),
        watch_dir: cfg.hot.watch_dir.map(|d| root.join(d)),
        lib_path: None, // defaults to target/<profile>/lib<package>.so
        auto_build: true,
        // Canonical set (scripts/hot-sets/<name>): must equal the hand-build
        // set or rival `riode` units break dylib loads. Never inline.
        companion_packages: riode_shared::hot_reload::companion_set(cfg.hot.companion_set),
    }
}

fn read_boot_chrome(config: &DevLoopConfig) -> HotWindowInfo {
    let mut loader = DylibReloader::new(config.lib_path());
    if !loader.refresh() {
        return HotWindowInfo::fallback();
    }
    let skewed = unsafe {
        match loader.get::<ComponentVersionFn>(b"riode_hot_version") {
            Ok(f) => f() != component_abi_fingerprint(),
            Err(_) => true,
        }
    };
    if skewed {
        return HotWindowInfo::fallback();
    }
    unsafe {
        match loader.get::<HotWindowFn>(b"riode_hot_window") {
            Ok(f) => f(),
            Err(_) => HotWindowInfo::fallback(),
        }
    }
}

/// Runs the overlay: connects, builds the hot UI once, presents it on a
/// layer-shell surface, and drives frames + hot-reload until killed.
pub fn overlay_run(cfg: &OverlayConfig) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let dev_root = std::env::var("RIODE_WORKSPACE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_dir().expect("overlay: cannot determine working directory (set RIODE_WORKSPACE_ROOT)")
        });

    // The shared runtime must be mapped globally BEFORE the UI image loads,
    // so the dylib resolves riode to this live image (single runtime).
    let shared_path = dev_root.join(format!("target/{}/libriode_shared.so", riode_shared::hot_reload::HOT_PROFILE));
    if let Err(e) = preload_shared(&shared_path) {
        eprintln!("{}: {e} (build it via `dev apps {}`)", cfg.namespace, cfg.namespace);
        std::process::exit(2);
    }

    // Blocking boot build, so the first frame already shows the real UI.
    let build_log = std::env::temp_dir().join(format!("{}-hot-boot-{}.log", cfg.namespace, std::process::id()));
    if let Err(tail) = blocking_hot_build(&dev_loop_config(cfg, dev_root.clone()), &build_log) {
        eprintln!("{}: hot-reload boot build failed; log: {}", cfg.namespace, build_log.display());
        for line in &tail {
            eprintln!("{}: hot build | {line}", cfg.namespace);
        }
        std::process::exit(1);
    }

    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm_state = Shm::bind(&globals, &qh)?;
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);
    let bg_manager = match bind_manager(&globals, &qh) {
        Ok(m) => {
            tracing::info!("{}: ext-background-effect-v1 available", cfg.namespace);
            Some(m)
        }
        Err(e) => {
            tracing::warn!("{}: no ext-background-effect-v1 ({e:?})", cfg.namespace);
            None
        }
    };
    let item_manager = match crate::item_hints::bind_item_manager(&globals, &qh) {
        Ok(m) => {
            tracing::info!("{}: riode-item-v1 available", cfg.namespace);
            Some(m)
        }
        Err(e) => {
            tracing::warn!("{}: no riode-item-v1 ({e:?})", cfg.namespace);
            None
        }
    };

    // Shared root factory: re-runs the CURRENT dylib entry on every
    // rebuild (hook state survives); placeholder until the first swap.
    let shared_root: Rc<Cell<Option<ComponentRootFn>>> = Rc::new(Cell::new(None));
    let factory_shared = Rc::clone(&shared_root);
    let factory: SharedFactory = Rc::new(move || {
        let Some(root) = factory_shared.get() else {
            return placeholder_root().into_element();
        };
        root()
    });

    // Window size comes from the hot chrome when available, else the
    // fallback: the layer-shell configure replaces it on the first frame.
    let chrome = read_boot_chrome(&dev_loop_config(cfg, dev_root.clone()));
    let (w, h) = (chrome.width.max(1), chrome.height.max(1));

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    runtime.borrow_mut().begin_build_window();
    let root = cx.enter_keyed("root", || factory()).into_element();
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, w, h);
    runtime.borrow_mut().end_build_window(true);
    engine.set_background(Color::TRANSPARENT);
    engine.load_font(cfg.font_bytes);
    engine.frame();
    let mut raster = HeadlessRaster::new();
    raster.draw_engine(&mut engine);

    let output = output_state.outputs().next();
    let layer = match cfg.surface {
        SurfacePolicy::Fullscreen => {
            create_overlay_surface(&compositor_state, &layer_shell, output.as_ref(), &qh, cfg.namespace)
        }
        SurfacePolicy::BottomFixed => {
            let wl_surface = compositor_state.create_surface(&qh);
            let layer = layer_shell.create_layer_surface(
                &qh,
                wl_surface,
                Layer::Overlay,
                Some(cfg.namespace.to_string()),
                output.as_ref(),
            );
            layer.set_anchor(Anchor::BOTTOM);
            layer.set_size(w, h);
            // Pure overlay: take no exclusive zone, steal no keyboard.
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            // Boot input region: rough centered rect; the first paint
            let region = compositor_state.wl_compositor().create_region(&qh, ());
            let rect = (
                (w as i32 - w as i32 * 90 / 100) / 2,
                (h as i32 - 72) / 2,
                w as i32 * 90 / 100,
                72.min(h as i32),
            );
            region.add(rect.0, rect.1, rect.2, rect.3);
            layer.wl_surface().set_input_region(Some(&region));
            region.destroy();
            layer.commit();
            layer
        }
    };

    // parent surface (the protocols allow at most one per surface).
    // Island-layers mode mints its own per island inside the pool sync.
    let (effect, item, island_bg, island_item) = if island_layers {
        (None, None, bg_manager, item_manager)
    } else {
        let effect = bg_manager
            .as_ref()
            .map(|m| effect_for_surface(m, &qh, layer.wl_surface()));
        let item = item_manager
            .as_ref()
            .map(|m| item_for_surface(m, &qh, layer.wl_surface()));
        (effect, item, None, None)
    };

    let pool = SlotPool::new((w * h * 4 * 2) as usize, &shm_state)?;

    let mut state = OverlayHost {
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        seat_state,
        output_state,
        compositor_state,
        layer_shell,
        shm_state,
        output: output.clone(),
        output_scale: 1,
        surface: Some(HostSurface {
            layer,
            pool,
            width: w,
            height: h,
            configured: false,
            waiting_for_frame: false,
        }),
        engine,
        raster,
        cfg: OverlayHostCfg {
            namespace: cfg.namespace.to_string(),
            interactive: cfg.interactive,
            island_layers,
            bottom_fixed: matches!(cfg.surface, SurfacePolicy::BottomFixed),
        },
        effect,
        item,
        island_bg,
        island_item,
        islands: IslandLayerPool::new(),
        input_rect: None,
        last_frame: None,
        last_island_log: Instant::now(),
        pointer_down: false,
        dev: DevLoop::new(dev_loop_config(cfg, dev_root)),
        shared_root,
        factory_installed: false,
        pending_activation: false,
        pending_island_paint: false,
    };
    // First hot lap before any frame: installs the freshly built UI
    // instead of flashing the placeholder.
    state.hot_poll();

    // Frame callbacks + pointer events both arrive here; while a transition
    // runs, each committed frame requests the next callback, so animation
    // advances without a timer. At rest nothing commits and we block.
    loop {
        event_queue.blocking_dispatch(&mut state)?;
    }
}
