// Single responsibility: WGPU device, adapter, queue, and swapchain configuration.

use std::sync::Arc;
use wgpu::{
    CompositeAlphaMode, Device, DeviceDescriptor, Instance, PresentMode, Queue,
    RequestAdapterOptions, Surface, SurfaceConfiguration, TextureFormat, TextureUsages,
};
use winit::window::Window;

/// Retained GPU adapter, device, and surface swapchain container.
pub struct GpuContext {
    /// Active WGPU logical device handle.
    pub device: Arc<Device>,
    /// Command execution submission queue.
    pub queue: Arc<Queue>,
    /// Native window presentation surface.
    pub surface: Surface<'static>,
    /// Swapchain surface configuration parameters.
    pub config: SurfaceConfiguration,
}

impl GpuContext {
    /// Initializes GPU adapter, device, and swapchain surface with optimal present mode.
    pub async fn init(window: Arc<Window>, width: u32, height: u32) -> Result<Self, String> {
        let instance = Instance::default();
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

        // Vello shaders perform internal sRGB conversion; select non-sRGB surface to avoid double gamma
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(
            caps.formats.first().copied().unwrap_or(TextureFormat::Bgra8Unorm),
        );

        // Prefer Mailbox for tear-free lowest latency; fallback to Immediate before AutoVsync for max throughput
        let present_mode = if caps.present_modes.contains(&PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else if caps.present_modes.contains(&PresentMode::Immediate) {
            PresentMode::Immediate
        } else {
            PresentMode::AutoVsync
        };

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1, // Single-frame latency for minimum input-to-photon delay
        };
        surface.configure(&device, &config);

        Ok(Self { device, queue, surface, config })
    }

    /// Reconfigures swapchain surface dimensions, clamping to non-zero physical limits.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }
}
