// Implements Smithay Client Toolkit protocol handler delegates with real VSync deltas.

use super::WaylandAppState;
use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::wlr_layer::{LayerShellHandler, LayerSurface},
    shm::{Shm, ShmHandler},
};
use std::time::Instant;
use wayland_client::{
    protocol::{wl_output, wl_seat, wl_surface},
    Connection, QueueHandle,
};

static mut LAST_FRAME: Option<Instant> = None;

delegate_compositor!(WaylandAppState);
delegate_layer!(WaylandAppState);
delegate_output!(WaylandAppState);
delegate_shm!(WaylandAppState);
delegate_seat!(WaylandAppState);
delegate_pointer!(WaylandAppState);
delegate_registry!(WaylandAppState);

impl ProvidesRegistryState for WaylandAppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl CompositorHandler for WaylandAppState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        let now = Instant::now();
        let dt = unsafe {
            let elapsed = LAST_FRAME.map_or(0.0166, |prev| (now - prev).as_secs_f32().min(0.05));
            LAST_FRAME = Some(now);
            elapsed
        };

        let is_animating = self.session.tick_animation(dt);
        if is_animating {
            self.surface.needs_redraw = true;
        }
        let session = &mut self.session;
        self.surface
            .on_frame_callback(qh, |b| session.render_to_buffer(b));
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

impl OutputHandler for WaylandAppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        let out = self.surface.target_output.clone();
        self.recreate_surface(out.as_ref());
    }
}

impl ShmHandler for WaylandAppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl LayerShellHandler for WaylandAppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        let out = self.surface.target_output.clone();
        self.recreate_surface(out.as_ref());
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        conf: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _: u32,
    ) {
        let (mut w, mut h) = conf.new_size;
        if w == 0 {
            w = 1920;
        }
        if h == 0 {
            h = 1080;
        }
        self.surface.is_configured = true;
        self.surface.set_dimensions(w, h, &self.shm_state);
        self.session
            .set_dimensions(self.surface.width, self.surface.height);
        let session = &mut self.session;
        self.surface.draw_frame(qh, |b| session.render_to_buffer(b));
    }
}

impl SeatHandler for WaylandAppState {
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
        if cap == Capability::Keyboard {
            seat.get_keyboard(qh, ());
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
