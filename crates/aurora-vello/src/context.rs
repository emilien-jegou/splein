// Single responsibility: WGPU device, adapter, queue, and swapchain configuration.

use std::sync::Arc;
use wgpu::{
    Backends, CompositeAlphaMode, Device, DeviceDescriptor, Extent3d, Instance, InstanceDescriptor,
    PresentMode, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
};
use winit::window::Window;

/// Retained GPU adapter, device, and surface swapchain container.
pub struct GpuContext {
    /// Active WGPU logical device handle.
    pub device: Arc<Device>,
    /// Command execution submission queue.
    pub queue: Arc<Queue>,
    /// Native window presentation surface. `None` for headless (quad-combine)
    /// contexts, which shade into the persistent target and read back.
    pub surface: Option<Surface<'static>>,
    /// Swapchain surface configuration parameters.
    pub config: SurfaceConfiguration,
    /// Persistent Vello render target, kept across frames.
    ///
    /// Vello renders the assembled scene here; only damaged rects are then
    /// blitted to the swapchain image. This saves present bandwidth (and
    /// compositor work) on partial updates, and is the staging point for
    /// future scissored tile re-render.
    pub persistent: Texture,
    /// View of the persistent render target for Vello.
    pub persistent_view: TextureView,
}

fn make_persistent(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("aurora-vello-persistent"),
        size: Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        // Vello's `render_to_texture` requires Rgba8Unorm + STORAGE_BINDING;
        // sRGB swapchain formats (e.g. Bgra8UnormSrgb) reject that usage.
        // The blit pass below samples this as a texture and converts to the
        // swapchain format, mirroring Vello's own `render_to_surface` path.
        // COPY_SRC feeds the quad-combine readback (texture -> staging
        // buffer for CPU compositing); unused by the standalone path.
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl GpuContext {
    /// Initializes GPU adapter, device, and swapchain surface with optimal present mode.
    pub async fn init(window: Arc<Window>, width: u32, height: u32) -> Result<Self, String> {
        // Primary APIs only: the GLES backend initializes EGL eagerly inside
        // `Instance::new`, and a broken EGL vendor setup panics there (khronos-egl
        // unwraps a missing error) instead of yielding no adapter. Vello targets
        // Vulkan/Metal/DX12 anyway, so GL is excluded rather than survived.
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| format!("Failed to create WGPU surface: {:?}", e))?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "No compatible GPU adapter found".to_string())?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("Failed to create GPU device: {:?}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let caps = surface.get_capabilities(&adapter);
        if caps.formats.is_empty() {
            return Err("GPU surface exposes no texture formats".to_string());
        }

        // Gamma-agnostic 2D compositing: scene bytes are sRGB-encoded and
        // must reach the compositor unmodified. Writing to an `*Srgb`
        // surface would hardware-encode linear->sRGB on store (double
        // gamma: 128 displays as ~188), so prefer plain Unorm formats and
        // raw-copy blit. Only if no Unorm format exists, take sRGB and let
        // the blit decode first so the hardware re-encode nets identity
        // (same ordering as egui_wgpu and vello's own util).
        let format = [TextureFormat::Bgra8Unorm, TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .or_else(|| caps.formats.iter().copied().find(|f| f.is_srgb()))
            .unwrap_or(
                caps.formats
                    .first()
                    .copied()
                    .unwrap_or(TextureFormat::Bgra8Unorm),
            );
        if format.is_srgb() {
            tracing::warn!(
                ?format,
                "no Unorm swapchain format available; blit will decode so the HW re-encode nets identity",
            );
        }

        // Mailbox first for tear-free lowest latency, then strict vsync. Immediate
        // tears, which reads as flicker on anything that moves — a UI toolkit must
        // never prefer it over a synchronized mode, so it is the last resort only.
        let present_mode = if caps.present_modes.contains(&PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else if caps.present_modes.contains(&PresentMode::Fifo) {
            PresentMode::Fifo
        } else if caps.present_modes.contains(&PresentMode::AutoVsync) {
            PresentMode::AutoVsync
        } else {
            PresentMode::Immediate
        };
        tracing::info!(
            mode = ?present_mode,
            available = ?caps.present_modes,
            format = ?format,
            "swapchain configured",
        );

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1, // Single-frame latency for minimum input-to-photon delay
        };
        surface.configure(&device, &config);
        let (persistent, persistent_view) = make_persistent(&device, config.width, config.height);

        Ok(Self {
            device,
            queue,
            surface: Some(surface),
            config,
            persistent,
            persistent_view,
        })
    }

    /// Initializes a headless GPU context: adapter + device with no window
    /// surface. The persistent shade target is sized directly; only the
    /// offscreen path (`render_scene_offscreen` + `readback_rgba8`) is
    /// usable — presenting requires a surface. Used by the quad-combine
    /// backend so several rasterizers share one window without opening one
    /// swapchain surface each (platforms allow a single surface per window).
    pub async fn init_headless(width: u32, height: u32) -> Result<Self, String> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: None,
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "No compatible GPU adapter found".to_string())?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("Failed to create GPU device: {:?}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        // No swapchain negotiates a format: the persistent target is always
        // Rgba8Unorm and the quad compositor consumes it directly.
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
            format: TextureFormat::Rgba8Unorm,
            width: width.max(1),
            height: height.max(1),
            present_mode: PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        let (persistent, persistent_view) = make_persistent(&device, config.width, config.height);
        Ok(Self {
            device,
            queue,
            surface: None,
            config,
            persistent,
            persistent_view,
        })
    }

    /// Reconfigures swapchain surface dimensions, clamping to non-zero physical limits.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
        // The persistent target must match the swapchain exactly or blits
        // fault: rebuild it (old contents are meaningless at a new size).
        let (persistent, persistent_view) =
            make_persistent(&self.device, self.config.width, self.config.height);
        self.persistent = persistent;
        self.persistent_view = persistent_view;
    }
}
