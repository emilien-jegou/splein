// Single responsibility: WGPU device, adapter, queue, and swapchain configuration.

use std::sync::Arc;
use wgpu::{
    Backends, CompositeAlphaMode, Device, DeviceDescriptor, Instance, InstanceDescriptor,
    PresentMode, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, TextureFormat,
    TextureUsages,
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

        // Vello shaders perform internal sRGB conversion; select non-sRGB surface to avoid double gamma
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(
                caps.formats
                    .first()
                    .copied()
                    .unwrap_or(TextureFormat::Bgra8Unorm),
            );

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

        Ok(Self {
            device,
            queue,
            surface,
            config,
        })
    }

    /// Reconfigures swapchain surface dimensions, clamping to non-zero physical limits.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }
}
