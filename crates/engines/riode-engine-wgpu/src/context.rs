// Single responsibility: WGPU device, adapter, queue, and swapchain configuration.

use std::sync::Arc;
use wgpu::{
    Adapter, Backends, CompositeAlphaMode, Device, DeviceDescriptor, Extent3d, Instance,
    InstanceDescriptor, InstanceFlags, PresentMode, Queue, Surface, SurfaceConfiguration, Texture,
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

fn select_backends() -> Backends {
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => Backends::VULKAN,
        Ok("metal") => Backends::METAL,
        Ok("dx12") => Backends::DX12,
        _ => Backends::PRIMARY,
    }
}

fn create_instance() -> (Instance, Backends) {
    let backends = select_backends();
    let instance = Instance::new(InstanceDescriptor {
        backends,
        flags: InstanceFlags::default() | InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
        ..Default::default()
    });
    (instance, backends)
}

/// Enumerates and scores all available adapters instead of relying on
/// `request_adapter`, which discards valid iGPUs if the window surface
/// has not completed its initial Wayland/X11 configuration.
fn select_best_adapter(
    instance: &Instance,
    surface: Option<&Surface<'_>>,
    backends: Backends,
) -> Result<Adapter, String> {
    let mut adapters: Vec<Adapter> = instance.enumerate_adapters(backends).into_iter().collect();

    if adapters.is_empty() {
        return Err(
            "No Vulkan GPU adapters found on this system.\n\
             If you are on Linux (ThinkPad):\n\
             - Ensure Vulkan drivers are installed: 'sudo apt install mesa-vulkan-drivers' (Debian/Ubuntu) or 'sudo pacman -S vulkan-intel' (Arch)\n\
             - Verify by running 'vulkaninfo' in your terminal."
                .to_string(),
        );
    }

    // Diagnostic logging: inspect everything WGPU sees on this laptop
    for (idx, a) in adapters.iter().enumerate() {
        let info = a.get_info();
        let surface_ok = surface.map_or(true, |s| a.is_surface_supported(s));
        tracing::info!(
            "[wgpu adapter {}] name={:?}, type={:?}, backend={:?}, surface_supported={}",
            idx,
            info.name,
            info.device_type,
            info.backend,
            surface_ok
        );
    }

    // Rank adapters:
    // +1000 if surface is confirmed supported
    // +500 Discrete GPU > +400 Integrated GPU > +300 Virtual GPU > +100 CPU / Software
    adapters.sort_by_key(|a| {
        let info = a.get_info();
        let mut score = 0i32;

        if surface.map_or(true, |s| a.is_surface_supported(s)) {
            score += 1000;
        }

        score += match info.device_type {
            wgpu::DeviceType::DiscreteGpu => 500,
            wgpu::DeviceType::IntegratedGpu => 400,
            wgpu::DeviceType::VirtualGpu => 300,
            wgpu::DeviceType::Cpu => 100,
            wgpu::DeviceType::Other => 50,
        };

        score
    });

    let best = adapters.pop().unwrap();
    tracing::info!(
        adapter = ?best.get_info().name,
        device_type = ?best.get_info().device_type,
        "Selected GPU adapter"
    );

    Ok(best)
}

async fn create_device(adapter: &Adapter) -> Result<(Device, Queue), String> {
    // Accommodate weak/older Intel iGPUs by capping requirements to downlevel defaults
    let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());

    adapter
        .request_device(
            &DeviceDescriptor {
                label: Some("riode-wgpu-device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            },
            None,
        )
        .await
        .map_err(|e| format!("Failed to create GPU device: {e:?}"))
}

fn make_persistent(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("riode-wgpu-persistent"),
        size: Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

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
        tracing::info!(
            ?picked,
            "sRGB swapchain: hardware OETF encodes on store (blit passthrough)"
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
        let (instance, backends) = create_instance();
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| format!("Failed to create WGPU surface: {e:?}"))?;

        let adapter = select_best_adapter(&instance, Some(&surface), backends)?;

        let (device, queue) = create_device(&adapter).await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let caps = surface.get_capabilities(&adapter);

        // Fall back gracefully if capabilities query occurs before window is mapped
        let format = if caps.formats.is_empty() {
            tracing::warn!("Surface exposed no formats (unmapped window). Falling back to Bgra8Unorm.");
            TextureFormat::Bgra8Unorm
        } else {
            pick_format(&caps.formats)
        };

        let present_mode = pick_present_mode(&caps.present_modes);

        let alpha_mode = if caps.alpha_modes.contains(&CompositeAlphaMode::Auto) {
            CompositeAlphaMode::Auto
        } else {
            caps.alpha_modes
                .first()
                .copied()
                .unwrap_or(CompositeAlphaMode::Opaque)
        };

        let mut usage = TextureUsages::RENDER_ATTACHMENT;
        if caps.usages.contains(TextureUsages::COPY_DST) {
            usage |= TextureUsages::COPY_DST;
        }

        tracing::info!(
            mode = ?present_mode,
            available = ?caps.present_modes,
            format = ?format,
            "riode-wgpu swapchain configured",
        );

        let config = SurfaceConfiguration {
            usage,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
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

    /// Initializes a headless GPU context: adapter + device with no window surface.
    pub async fn init_headless(width: u32, height: u32) -> Result<Self, String> {
        let (instance, backends) = create_instance();
        let adapter = select_best_adapter(&instance, None, backends)?;

        let (device, queue) = create_device(&adapter).await?;
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
            desired_maximum_frame_latency: 2,
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
