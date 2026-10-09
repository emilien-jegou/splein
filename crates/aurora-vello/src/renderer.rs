// Single responsibility: Vello GPU compute rasterizer coordinating swapchain surface frames.

use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect};
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer as VelloEngine, RendererOptions,
    Scene as VelloScene,
};
use winit::window::Window;

use aurora::foundation::{Color, DamageRegion, Point, ResolvedRect, Transform};
use aurora::render::error::BackendError;
use aurora::scene::chunk::{InheritedClips, SceneChunk};
use aurora::scene::command::{hash_commands, transform_bits};
use vello::peniko::BlendMode;
use crate::command::{compile_vello_chunk, VelloImageCache};
use crate::context::GpuContext;
use crate::shader::to_vello_color;
use crate::shadow::VelloShadowCache;
use crate::svg::VelloSvgCache;
use crate::text::VelloGlyphCache;
use aurora::scene::scene::Scene;
use aurora::text::TextContext;

/// Pure reuse predicate, headless-testable: a chunk is re-encodable from
/// cache iff damage is non-empty (known), small enough to cull, and no
/// damage rect overlaps the chunk bounds.
#[allow(dead_code)]
pub fn chunk_reusable(
    bounds: &ResolvedRect,
    damage: &DamageRegion,
    damage_area: f32,
    viewport_area: f32,
) -> bool {
    if damage.is_empty() {
        return false;
    }
    if damage_area >= 0.6 * viewport_area {
        return false;
    }
    !damage.rects().iter().any(|d| d.overlaps(bounds))
}

/// Per-frame antialiasing selection.
///
/// `Area` is analytic high quality and the best-in-class choice for UI:
/// every curve and glyph edge gets exact coverage instead of MSAA sample
/// patterns. It is the most expensive coarse work, but quality outranks
/// cost here — rect/text/shadow UI all render through it uniformly.
pub fn aa_for_chunks<'a>(chunks: impl Iterator<Item = &'a SceneChunk>) -> AaConfig {
    let _ = chunks;
    AaConfig::Area
}

/// Placement affine for a chunk: translation to its origin composed with the
/// hoisted stacking-root transform. The fragment stream is chunk-local, so
/// this is what puts it back on screen.
pub fn chunk_placement_affine(origin: &Point, placement: &Transform) -> Affine {
    Affine::translate((origin.x as f64, origin.y as f64))
        * Affine::new([
            placement.a as f64,
            placement.b as f64,
            placement.c as f64,
            placement.d as f64,
            placement.tx as f64,
            placement.ty as f64,
        ])
}

/// Viewport bound expressed in a fragment's local space, for opacity layers.
///
/// The fragment is appended under `placement`, so a layer bound given in
/// screen space would be transformed twice. Mapping the viewport through the
/// inverse placement keeps the layer covering exactly the screen. Degenerate
/// placements (zero scale) are invisible anyway — a giant bound is safe.
pub fn local_viewport_bound(viewport: &KurboRect, placement: &Affine) -> KurboRect {
    let [pa, pb, pc, pd, _, _] = placement.as_coeffs();
    let det = pa * pd - pb * pc;
    if det.abs() < 1e-9 {
        return KurboRect::new(-1e6, -1e6, 1e6, 1e6);
    }
    let inv = placement.inverse();
    let corners = [
        inv * vello::kurbo::Point::new(viewport.x0, viewport.y0),
        inv * vello::kurbo::Point::new(viewport.x1, viewport.y0),
        inv * vello::kurbo::Point::new(viewport.x0, viewport.y1),
        inv * vello::kurbo::Point::new(viewport.x1, viewport.y1),
    ];
    let (mut x0, mut y0, mut x1, mut y1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in corners {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    KurboRect::new(x0, y0, x1, y1)
}
/// Cached Vello encoding of one chunk's content, reusable across moves.
///
/// Identity is placement-POSITION-free: `content_hash` covers the
/// chunk-local stream and `placement_bits` the hoisted root transform, so a
/// scrolled/translated chunk (same content, new origin) re-appends instead
/// of recompiling. Origin travels in the append transform, never in identity.
struct ChunkFragment {
    content_hash: u64,
    placement_bits: [u32; 6],
    scene: VelloScene,
}

/// Blit pipeline sampling the linear Rgba8Unorm persistent target onto the
/// (sRGB) swapchain surface. Mirrors Vello's own `render_to_surface` blit,
/// plus `set_scissor_rect` per damage rect for partial presents.
fn create_blit_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    const SHADERS: &str = r#"
        @vertex
        fn vs_main(@builtin(vertex_index) ix: u32) -> @builtin(position) vec4<f32> {
            var vertex = vec2<f32>(-1.0, 1.0);
            switch ix {
                case 1u: { vertex = vec2<f32>(-1.0, -1.0); }
                case 2u, 4u: { vertex = vec2<f32>(1.0, -1.0); }
                case 5u: { vertex = vec2<f32>(1.0, 1.0); }
                default: {}
            }
            return vec4(vertex, 0.0, 1.0);
        }
        // Raw copy: shader vec4(r,g,b,a) addresses logical channels and the
        // surface format describes memory layout, so no manual R/B swizzle
        // is needed (or correct) for BGRA targets.
        //
        // Transfer: scene bytes are already sRGB-encoded. On a plain Unorm
        // surface the copy below is exact (no hardware conversion). Only
        // when forced onto an sRGB surface (see context format fallback),
        // SRGB_DECODE applies the EOTF first so the hardware OETF on store
        // nets back to identity instead of double-encoding.
        override SRGB_DECODE: bool = false;
        @group(0) @binding(0)
        var fine_output: texture_2d<f32>;
        fn srgb_eotf(c: vec3<f32>) -> vec3<f32> {
            let lo = c / 12.92;
            let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
            return select(hi, lo, c <= vec3<f32>(0.04045));
        }
        @fragment
        fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
            let rgba_sep = textureLoad(fine_output, vec2<i32>(pos.xy), 0);
            var rgb = rgba_sep.rgb * rgba_sep.a;
            if (SRGB_DECODE) {
                rgb = srgb_eotf(rgb);
            }
            return vec4(rgb, rgba_sep.a);
        }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("aurora-vello-blit"),
        source: wgpu::ShaderSource::Wgsl(SHADERS.into()),
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("aurora-vello-blit-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::FRAGMENT,
            binding: 0,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("aurora-vello-blit-layout-pipe"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("aurora-vello-blit-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &std::collections::HashMap::from([(
                    "SRGB_DECODE".to_string(),
                    if surface_format.is_srgb() { 1.0 } else { 0.0 },
                )]),
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });
    (bind_layout, pipeline)
}

/// High-throughput GPU rasterizer executing compute shaders over display lists.
pub struct VelloRenderer {
    _window: Arc<Window>,
    gpu: GpuContext,
    vello_engine: VelloEngine,
    vello_scene: VelloScene,
    text_ctx: TextContext,
    tx_stack: Vec<Affine>,
    shadow_cache: VelloShadowCache,
    svg_cache: VelloSvgCache,
    image_cache: VelloImageCache,
    glyph_cache: VelloGlyphCache,
    viewport_bound: KurboRect,
    blit_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    fragments: Vec<ChunkFragment>,
    /// Chunks re-encoded vs appended-from-cache last frame (telemetry).
    pub last_encoded: usize,
    pub last_reused: usize,
    /// Damage blits issued to the swapchain last frame (telemetry).
    pub last_blit_rects: usize,
    pub last_blit_bytes: u64,
    /// Cached readback staging buffer (size + buffer) for offscreen use.
    readback: Option<(u32, u32, wgpu::Buffer)>,
}

impl VelloRenderer {
    /// Creates a Vello compute renderer configured for the target OS window surface.
    pub fn new(window: Arc<Window>, width: u32, height: u32) -> Result<Self, BackendError> {
        Self::new_inner(window, width, height, false)
    }

    /// Creates a Vello renderer with CPU-staged coarse rasterization.
    ///
    /// Presentation still goes through wgpu (fine raster + blit stay on the
    /// GPU); only the stages up to fine rasterization run on the CPU. Same
    /// scene input and same swapchain output as [`Self::new`], so frames are
    /// comparable across the two engines.
    pub fn new_cpu(window: Arc<Window>, width: u32, height: u32) -> Result<Self, BackendError> {
        Self::new_inner(window, width, height, true)
    }

    fn new_inner(
        window: Arc<Window>,
        width: u32,
        height: u32,
        use_cpu: bool,
    ) -> Result<Self, BackendError> {
        let gpu = pollster::block_on(GpuContext::init(Arc::clone(&window), width, height))
            .map_err(BackendError::init)?;
        let vello_engine = VelloEngine::new(
            &gpu.device,
            RendererOptions {
                surface_format: Some(gpu.config.format),
                use_cpu,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        )
        .map_err(|e| BackendError::init(format!("Failed to initialize Vello engine: {e:?}")))?;

        let viewport_bound = KurboRect::new(0.0, 0.0, width.max(1) as f64, height.max(1) as f64);
        let (blit_layout, blit_pipeline) =
            create_blit_pipeline(&gpu.device, gpu.config.format);
        Ok(Self {
            _window: window,
            gpu,
            vello_engine,
            vello_scene: VelloScene::new(),
            text_ctx: TextContext::new(),
            tx_stack: Vec::with_capacity(32),
            shadow_cache: VelloShadowCache::new(),
            svg_cache: VelloSvgCache::new(),
            image_cache: VelloImageCache::new(),
            glyph_cache: VelloGlyphCache::new(),
            viewport_bound,
            blit_layout,
            blit_pipeline,
            fragments: Vec::new(),
            last_encoded: 0,
            last_reused: 0,
            last_blit_rects: 0,
            last_blit_bytes: 0,
            readback: None,
        })
    }

    /// Updates the typography shaper and glyph database handle.
    pub fn set_text_context(&mut self, text_ctx: TextContext) {
        self.text_ctx = text_ctx;
    }

    /// Resizes the WGPU swapchain and presentation bounds.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.viewport_bound = KurboRect::new(0.0, 0.0, width.max(1) as f64, height.max(1) as f64);
    }

    /// Encodes the scene and shades it into the persistent texture.
    ///
    /// Same work as [`Self::render_frame`] minus the swapchain acquire, blit
    /// and present: the quad-combine backend reads the persistent texture
    /// back to CPU (see [`Self::readback_rgba8`]) and composites it with the
    /// other engines into one window. Damage selects encode reuse exactly as
    /// in the standalone path; shading itself is always fullscreen.
    #[tracing::instrument(name = "Vello::RenderOffscreen", skip_all)]
    pub fn render_scene_offscreen(
        &mut self,
        scene: &Scene,
        base_color: Color,
        _damage: &DamageRegion,
    ) -> Result<usize, BackendError> {
        // Damage selects encode reuse below; shading is always fullscreen.
        // A damaged-but-identical chunk (e.g. scrolled content) still
        // re-appends instead of recompiling.
        self.vello_scene.reset();
        self.tx_stack.clear();

        let viewport = self.viewport_rect();
        let mut encoded = 0usize;
        let mut reused = 0usize;

        // Align fragment cache with current topology; stale entries are reset.
        if self.fragments.len() != scene.chunks.len() {
            self.fragments.clear();
            for _ in &scene.chunks {
                self.fragments.push(ChunkFragment {
                    content_hash: u64::MAX,
                    placement_bits: [u32::MAX; 6],
                    scene: VelloScene::new(),
                });
            }
        }

        for (i, chunk) in scene.chunks.iter().enumerate() {
            // Viewport cull always applies: off-screen chunks encode nothing.
            if !chunk.bounds.overlaps(&viewport) {
                let frag = &mut self.fragments[i];
                frag.scene.reset();
                frag.content_hash = u64::MAX;
                continue;
            }
            // Snapshot identity without holding the borrow across compile.
            let identity_matches = {
                let frag = &self.fragments[i];
                let hash = hash_commands(&chunk.commands);
                let bits = transform_bits(&chunk.placement);
                frag.content_hash == hash && frag.placement_bits == bits
            };
            if identity_matches {
                let placement_affine =
                    chunk_placement_affine(&chunk.abs_origin, &chunk.placement);
                self.assemble_chunk(i, &chunk.inherited_clips, &placement_affine);
                reused += 1;
                continue;
            }
            // Recompile content into an isolated fragment, then assemble.
            let placement_affine =
                chunk_placement_affine(&chunk.abs_origin, &chunk.placement);
            let local_bound = local_viewport_bound(&self.viewport_bound, &placement_affine);
            let mut frag_scene = VelloScene::new();
            compile_vello_chunk()
                .commands(&chunk.commands)
                .scene(&mut frag_scene)
                .bound(&local_bound)
                .tx_stack(&mut self.tx_stack)
                .shadow_cache(&mut self.shadow_cache)
                .svg_cache(&mut self.svg_cache)
                .image_cache(&mut self.image_cache)
                .glyph_cache(&mut self.glyph_cache)
                .text_ctx(&self.text_ctx)
                .call();
            encoded += 1;
            let frag = &mut self.fragments[i];
            frag.scene = frag_scene;
            frag.content_hash = hash_commands(&chunk.commands);
            frag.placement_bits = transform_bits(&chunk.placement);
            self.assemble_chunk(i, &chunk.inherited_clips, &placement_affine);
        }
        self.last_encoded = encoded;
        self.last_reused = reused;

        // Shade into the persistent target (fullscreen; see render_frame for
        // why partial presents can't Load a rotated swapchain image here —
        // the offscreen consumer reads this texture instead).
        let aa = aa_for_chunks(scene.chunks.iter());
        self.vello_engine
            .render_to_texture(
                &self.gpu.device,
                &self.gpu.queue,
                &self.vello_scene,
                &self.gpu.persistent_view,
                &RenderParams {
                    base_color: to_vello_color(base_color, 1.0),
                    width: self.gpu.config.width,
                    height: self.gpu.config.height,
                    antialiasing_method: aa,
                },
            )
            .map_err(|e| {
                BackendError::raster(format!("Vello GPU render execution failed: {e:?}"))
            })?;
        Ok(encoded)
    }

    /// Persistent target size in pixels (quadrant size in quad-combine use).
    pub fn target_size(&self) -> (u32, u32) {
        (self.gpu.config.width.max(1), self.gpu.config.height.max(1))
    }

    /// Copies the persistent texture to CPU as tightly-packed RGBA bytes.
    ///
    /// Must follow [`Self::render_scene_offscreen`] once the GPU work is
    /// submitted (render_to_texture submits internally; this only adds the
    /// copy + a blocking map). Staging buffer is cached across frames and
    /// rebuilt on size change. The quad-combine backend composites these
    /// bytes with the other engines into one window.
    #[tracing::instrument(name = "Vello::Readback", skip_all)]
    pub fn readback_rgba8(&mut self) -> Result<Vec<u8>, BackendError> {
        let (w, h) = self.target_size();
        // COPY_BYTES_PER_ROW_ALIGNMENT: rows are padded to 256B.
        let pitch = ((w * 4 + 255) / 256 * 256) as usize;
        let size = (pitch * h as usize) as u64;
        let buf = match &self.readback {
            Some((bw, bh, b)) if *bw == w && *bh == h => b,
            _ => {
                let b = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("aurora-vello-readback"),
                    size: size.max(256),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                self.readback = Some((w, h, b));
                &self.readback.as_ref().unwrap().2
            }
        };
        let mut encoder =
            self.gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("aurora-vello-readback"),
                });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.gpu.persistent,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch as u32),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(Some(encoder.finish()));

        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| BackendError::raster("readback map channel closed"))?
            .map_err(|e| BackendError::raster(format!("readback map failed: {e:?}")))?;
        let out = {
            let mapped = slice.get_mapped_range();
            let mut tight = vec![0u8; (w * h * 4) as usize];
            for y in 0..h as usize {
                tight[y * w as usize * 4..(y + 1) * w as usize * 4]
                    .copy_from_slice(&mapped[y * pitch..y * pitch + w as usize * 4]);
            }
            tight
        };
        buf.unmap();
        Ok(out)
    }

    /// Renders display list scene to the swapchain and presents.
    ///
    /// Encode reuse is content-addressed: each chunk compiles into an
    /// isolated fragment keyed by its command hash, so identical content
    /// re-appends under the current placement instead of recompiling — even
    /// when the chunk moved (scroll/offset) or Aurora recompiled it for
    /// layout-local reasons. Every visible chunk still contributes each frame
    /// (shade is fullscreen via `render_to_surface`). Returns chunks
    /// re-encoded (telemetry).
    #[tracing::instrument(name = "Vello::RenderFrame", skip_all)]
    pub fn render_frame(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<usize, BackendError> {
        let encoded = self.render_scene_offscreen(scene, base_color, damage)?;

        let surface_texture = match self.gpu.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.gpu
                    .surface
                    .configure(&self.gpu.device, &self.gpu.config);
                self.gpu.surface.get_current_texture().map_err(|e| {
                    BackendError::surface(format!("swapchain recovery failed: {e:?}"))
                })?
            }
            Err(e) => {
                return Err(BackendError::surface(format!(
                    "swapchain acquire failed: {e:?}"
                )))
            }
        };

        let (sw, sh) = (self.gpu.config.width, self.gpu.config.height);
        {
            let surface_view = surface_texture.texture.create_view(
                &wgpu::TextureViewDescriptor::default(),
            );
            let bind_group =
                self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("aurora-vello-blit-bg"),
                    layout: &self.blit_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &self.gpu.persistent_view,
                        ),
                    }],
                });
            let mut encoder = self.gpu.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("aurora-vello-blit"),
                },
            );
            {
                let mut pass =
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("aurora-vello-blit-pass"),
                        color_attachments: &[Some(
                            wgpu::RenderPassColorAttachment {
                                view: &surface_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            },
                        )],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });
                pass.set_pipeline(&self.blit_pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
            self.gpu.queue.submit(Some(encoder.finish()));
        }
        self.last_blit_rects = 1;
        self.last_blit_bytes = (sw as u64) * (sh as u64) * 4;

        surface_texture.present();
        Ok(encoded)
    }

    fn viewport_rect(&self) -> aurora::foundation::ResolvedRect {
        aurora::foundation::ResolvedRect::new(
            0.0,
            0.0,
            self.gpu.config.width.max(1) as f32,
            self.gpu.config.height.max(1) as f32,
        )
    }

    /// Assembles one cached fragment into the frame scene under current placement.
    ///
    /// Inherited clips are placement-time state: pushed absolute around the
    /// placed fragment, so a moved chunk reuses its content encoding while
    /// clipping stays exactly where the ancestors are now.
    fn assemble_chunk(&mut self, frag_idx: usize, clips: &InheritedClips, placement: &Affine) {
        for clip in clips.iter() {
            let shape = crate::path::build_kurbo_rounded_rect(&clip.rect, clip.radius);
            self.vello_scene.push_layer(
                BlendMode::default(),
                1.0,
                Affine::IDENTITY,
                &shape,
            );
        }
        let frag = &self.fragments[frag_idx];
        self.vello_scene.append(&frag.scene, Some(*placement));
        for _ in clips.iter() {
            self.vello_scene.pop_layer();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora::foundation::ResolvedRect;
    use aurora::scene::SceneCommand;

    fn damage_with(rects: &[(f32, f32, f32, f32)]) -> DamageRegion {
        let mut d = DamageRegion::new();
        for &(x, y, w, h) in rects {
            d.push(ResolvedRect::new(x, y, w, h));
        }
        d
    }

    #[test]
    fn untouched_chunk_is_reusable() {
        let bounds = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
        let damage = damage_with(&[(500.0, 500.0, 10.0, 10.0)]);
        assert!(chunk_reusable(&bounds, &damage, 100.0, 800.0 * 600.0));
    }

    #[test]
    fn touched_chunk_is_not_reusable() {
        let bounds = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
        let damage = damage_with(&[(50.0, 50.0, 10.0, 10.0)]);
        assert!(!chunk_reusable(&bounds, &damage, 100.0, 800.0 * 600.0));
    }

    #[test]
    fn empty_damage_never_reuses() {
        let bounds = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
        let damage = DamageRegion::new();
        assert!(!chunk_reusable(&bounds, &damage, 0.0, 800.0 * 600.0));
    }

    #[test]
    fn fullscreen_damage_disables_reuse() {
        let bounds = ResolvedRect::new(0.0, 0.0, 100.0, 100.0);
        let damage = damage_with(&[(0.0, 0.0, 800.0, 600.0)]);
        assert!(!chunk_reusable(&bounds, &damage, 480_000.0, 480_000.0));
    }

    fn sample_commands() -> Vec<SceneCommand> {
        vec![
            SceneCommand::PushOffset(Point::new(0.0, 0.0)),
            SceneCommand::DrawRect {
                rect: ResolvedRect::new(0.0, 0.0, 100.0, 40.0),
                appearance: aurora::foundation::Appearance {
                    fill: Some(aurora::foundation::Fill::Solid(aurora::foundation::Color::BLACK)),
                    stroke: None,
                    radius: aurora::foundation::Radius::Scalar(4.0),
                    shadow: None,
                    light_override: None,
                    opacity: 1.0,
                },
            },
            SceneCommand::PopOffset,
        ]
    }

    #[test]
    fn identical_streams_hash_equal_across_origins() {
        // Origin is placement, not content: the same stream at two origins
        // must hash identically or scroll reuse never hits.
        let a = sample_commands();
        let b = sample_commands();
        assert_eq!(hash_commands(&a), hash_commands(&b));
    }

    #[test]
    fn changed_fill_changes_hash() {
        let mut b = sample_commands();
        if let SceneCommand::DrawRect { appearance, .. } = &mut b[1] {
            appearance.opacity = 0.5;
        }
        assert_ne!(hash_commands(&sample_commands()), hash_commands(&b));
    }

    #[test]
    fn placement_affine_matches_legacy_stream_order() {
        // Legacy stream applied PushOffset(origin) then PushTransform(R):
        // p_screen = T(origin) * R * p_local. Placement must reproduce it.
        let origin = Point::new(30.0, 20.0);
        let placement = Transform {
            tx: 5.0,
            ..Transform::IDENTITY
        };
        let affine = chunk_placement_affine(&origin, &placement);
        let p = affine * vello::kurbo::Point::new(1.0, 1.0);
        assert!((p.x - 36.0).abs() < 1e-6 && (p.y - 21.0).abs() < 1e-6);
    }

    #[test]
    fn degenerate_placement_gets_safe_bound() {
        let zero = Affine::new([0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let vp = KurboRect::new(0.0, 0.0, 800.0, 600.0);
        let local = local_viewport_bound(&vp, &zero);
        assert!(local.width() > 1e5);
    }

    fn svg_chunk() -> SceneChunk {
        let mut c = SceneChunk::new(
            ResolvedRect::new(0.0, 0.0, 10.0, 10.0),
            Point::new(0.0, 0.0),
        );
        c.push(aurora::scene::SceneCommand::DrawSvg {
            rect: ResolvedRect::new(0.0, 0.0, 10.0, 10.0),
            graphic: aurora::scene::VectorGraphic::from_str(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>",
            )
            .unwrap(),
        });
        c
    }

    #[test]
    fn svg_forces_area_aa() {
        let plain = sample_commands();
        let svg = svg_chunk();
        let chunks = vec![
            SceneChunk::new(ResolvedRect::new(0.0, 0.0, 1.0, 1.0), Point::new(0.0, 0.0)),
            svg,
        ];
        assert!(matches!(
            aa_for_chunks(chunks.iter()),
            AaConfig::Area
        ));
        let _ = plain;
    }

    #[test]
    fn rect_ui_selects_area() {
        let mut c = SceneChunk::new(
            ResolvedRect::new(0.0, 0.0, 100.0, 40.0),
            Point::new(0.0, 0.0),
        );
        c.commands = sample_commands().into();
        let chunks = vec![c];
        assert!(matches!(
            aa_for_chunks(chunks.iter()),
            AaConfig::Area
        ));
    }
}
