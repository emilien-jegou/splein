// Single responsibility: WGPU device, adapter, queue, and swapchain configuration.

use std::sync::Arc;
use wgpu::{
    Backends, CompositeAlphaMode, Device, DeviceDescriptor, Extent3d, Instance, InstanceDescriptor,
    PresentMode, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
};
use winit::window::Window;

/// Retained GPU adapter, device, and surface swapchain container.
pub struct WgpuContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    /// Native window presentation surface. `None` for headless (quad-combine)
    /// contexts, which shade into the persistent target and read back.
    pub surface: Option<Surface<'static>>,
    pub config: SurfaceConfiguration,
    pub persistent: Texture,
    pub persistent_view: TextureView,
}

fn make_persistent(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("aurora-wgpu-persistent"),
        size: Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Gamma-agnostic format pick: scene bytes are sRGB-encoded and must
/// reach the compositor unmodified. Writing to an `*Srgb` surface would
/// hardware-encode linear->sRGB on store (double gamma), so prefer plain
/// Unorm formats. Only if none exists, take sRGB and let the blit decode
/// first so the hardware re-encode nets identity.
fn pick_format(formats: &[TextureFormat]) -> TextureFormat {
    let picked = [TextureFormat::Bgra8Unorm, TextureFormat::Rgba8Unorm]
        .into_iter()
        .find(|f| formats.contains(f))
        .or_else(|| formats.iter().copied().find(|f| !f.is_srgb()))
        .or_else(|| formats.iter().copied().find(|f| f.is_srgb()))
        .unwrap_or(
            formats
                .first()
                .copied()
                .unwrap_or(TextureFormat::Bgra8Unorm),
        );
    if picked.is_srgb() {
        tracing::warn!(
            ?picked,
            "no Unorm swapchain format available; blit will decode so the HW re-encode nets identity",
        );
    }
    picked
}

fn pick_present_mode(modes: &[PresentMode]) -> PresentMode {
    if modes.contains(&PresentMode::Mailbox) {
        PresentMode::Mailbox
    } else if modes.contains(&PresentMode::Fifo) {
        PresentMode::Fifo
    } else if modes.contains(&PresentMode::AutoVsync) {
        PresentMode::AutoVsync
    } else {
        PresentMode::Immediate
    }
}

impl WgpuContext {
    pub async fn init(window: Arc<Window>, width: u32, height: u32) -> Result<Self, String> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| format!("Failed to create WGPU surface: {e:?}"))?;
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
            .map_err(|e| format!("Failed to create GPU device: {e:?}"))?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let caps = surface.get_capabilities(&adapter);
        if caps.formats.is_empty() {
            return Err("GPU surface exposes no texture formats".to_string());
        }
        let format = pick_format(&caps.formats);
        let present_mode = pick_present_mode(&caps.present_modes);
        tracing::info!(
            mode = ?present_mode,
            available = ?caps.present_modes,
            format = ?format,
            "aurora-wgpu swapchain configured",
        );
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
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
    /// surface. Only the offscreen path (`render_scene_offscreen` +
    /// `readback_rgba8`) is usable. Used by the quad-combine backend so
    /// several rasterizers share one window without opening one swapchain
    /// surface each (platforms allow a single surface per window).
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
            .map_err(|e| format!("Failed to create GPU device: {e:?}"))?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
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

    pub fn resize(&mut self, width: u32, height: u32) {
        let (nw, nh) = (width.max(1), height.max(1));
        if self.config.width == nw && self.config.height == nh {
            return;
        }
        self.config.width = nw;
        self.config.height = nh;
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
        let (persistent, persistent_view) = make_persistent(&self.device, nw, nh);
        self.persistent = persistent;
        self.persistent_view = persistent_view;
    }
}
