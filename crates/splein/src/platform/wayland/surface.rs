// crates/splein/src/platform/wayland/surface.rs

use super::WaylandAppState;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

pub struct OverlaySurface {
    layer: LayerSurface,
    pool: SlotPool,
    pub width: u32,
    pub height: u32,
    pub needs_redraw: bool,
    pub waiting_for_frame: bool,
}

impl OverlaySurface {
    pub fn new(
        compositor: &CompositorState,
        layer_shell: &LayerShell,
        shm: &Shm,
        qh: &QueueHandle<WaylandAppState>,
    ) -> eyre::Result<Self> {
        let surface = compositor.create_surface(qh);
        let layer = layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("presentify-overlay"),
            None,
        );

        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);

        // Configure initial passthrough input region BEFORE committing the surface
        let empty_region = compositor.wl_compositor().create_region(qh, ());
        layer.wl_surface().set_input_region(Some(&empty_region));
        empty_region.destroy();

        // Exactly ONE commit to request the layer surface from compositor
        layer.commit();

        let initial_width = 1920;
        let initial_height = 1080;
        let pool = SlotPool::new(
            (initial_width * initial_height * 4 * 3) as usize,
            shm,
        )?;

        eprintln!("[splein] Layer surface requested (Overlay tier, passthrough input)");

        Ok(Self {
            layer,
            pool,
            width: initial_width,
            height: initial_height,
            needs_redraw: false,
            waiting_for_frame: false,
        })
    }

    pub fn set_dimensions(&mut self, width: u32, height: u32, shm: &Shm) {
        if width > 0 && height > 0 && (self.width != width || self.height != height) {
            self.width = width;
            self.height = height;
            let required_bytes = (width * height * 4 * 3) as usize;
            if let Ok(new_pool) = SlotPool::new(required_bytes, shm) {
                self.pool = new_pool;
            }
            eprintln!("[splein] Surface resized to {}x{}", width, height);
        }
        self.waiting_for_frame = false;
    }

    pub fn set_passthrough(
        &mut self,
        passthrough: bool,
        compositor: &CompositorState,
        qh: &QueueHandle<WaylandAppState>,
    ) {
        let wl_surf = self.layer.wl_surface();
        if passthrough {
            let empty_region = compositor.wl_compositor().create_region(qh, ());
            wl_surf.set_input_region(Some(&empty_region));
            empty_region.destroy();
            self.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        } else {
            wl_surf.set_input_region(None);
            self.layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        }
        wl_surf.commit();
    }

    pub fn request_redraw<F>(&mut self, qh: &QueueHandle<WaylandAppState>, render: F)
    where
        F: FnOnce(&mut [u8]),
    {
        self.needs_redraw = true;
        if !self.waiting_for_frame {
            self.draw_frame(qh, render);
        }
    }

    pub fn on_frame_callback<F>(&mut self, qh: &QueueHandle<WaylandAppState>, render: F)
    where
        F: FnOnce(&mut [u8]),
    {
        self.waiting_for_frame = false;
        if self.needs_redraw {
            self.draw_frame(qh, render);
        }
    }

    pub fn draw_frame<F>(&mut self, qh: &QueueHandle<WaylandAppState>, render: F)
    where
        F: FnOnce(&mut [u8]),
    {
        let (width, height) = (self.width, self.height);
        if width == 0 || height == 0 {
            eprintln!("[splein] Cannot draw frame: dimensions are {}x{}", width, height);
            return;
        }

        let stride = width * 4;
        let (buffer, canvas) = match self.pool.create_buffer(
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        ) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[splein] Error: Failed to allocate SHM buffer ({}x{}): {:?}", width, height, e);
                return;
            }
        };

        render(canvas);
        self.needs_redraw = false;

        let wl_surf = self.layer.wl_surface();
        wl_surf.frame(qh, wl_surf.clone());
        self.waiting_for_frame = true;

        wl_surf.attach(Some(buffer.wl_buffer()), 0, 0);
        wl_surf.damage_buffer(0, 0, width as i32, height as i32);
        wl_surf.commit();
    }
}
