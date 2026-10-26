// Single responsibility: Main render loop with batched rect + glyph scene pass + blit.

use riode::foundation::{Color, DamageRegion, ResolvedRect};
use riode::foundation::glass::light_spill_replay;
use riode::render::BackendError;
use riode::scene::scene::Scene;
use riode::text::TextContext;
use std::sync::Arc;
use wgpu::util::DeviceExt;

use crate::cache::{chunk_key, ChunkCache};
use crate::command::{Batch, FilterGpu, Op, StageGpu};
use crate::context::WgpuContext;
use crate::image::ImageCache;
use crate::pipeline::{
    create_pipelines, BlurGpu, ColorMatrixGpu, GlassFrameGpu, GlyphVertex, ImageVertex, Pipelines,
    RRectVertex, RectVertex, ShadowVertex,
};
use crate::svg::SvgCache;
use crate::text::GlyphAtlas;

fn to_wgpu(c: Color) -> wgpu::Color {
    // The persistent target is plain Unorm: clearValue stores verbatim,
    // so the sRGB-encoded CPU color rides through unchanged.
    wgpu::Color {
        r: c.r as f64,
        g: c.g as f64,
        b: c.b as f64,
        a: c.a as f64,
    }
}

pub struct WgpuRenderer {
    gpu: WgpuContext,
    pipes: Pipelines,
    batch: Batch,
    atlas: GlyphAtlas,
    text_ctx: TextContext,
    ortho_buf: wgpu::Buffer,
    ortho_bg: wgpu::BindGroup,
    glyph_bg_linear: wgpu::BindGroup,
    glyph_bg_nearest: wgpu::BindGroup,
    vert_buf: wgpu::Buffer,
    vert_cap: u64,
    glyph_buf: wgpu::Buffer,
    glyph_cap: u64,
    rrect_buf: wgpu::Buffer,
    rrect_cap: u64,
    shadow_buf: wgpu::Buffer,
    shadow_cap: u64,
    image_buf: wgpu::Buffer,
    image_cap: u64,
    images: ImageCache,
    svgs: SvgCache,
    scale: f32,
    chunks: ChunkCache,
    /// Force a Clear of the persistent target on the next frame. Set at
    /// construction and whenever resize rebuilds the target (fresh texture
    /// memory is undefined). Without this, a Load would preserve garbage.
    clear_next: bool,
    /// Cached readback staging buffer (width, height, buffer) for offscreen use.
    readback: Option<(u32, u32, wgpu::Buffer)>,
    /// Filter-layer offscreen pool, one slot per open nesting depth.
    /// Surface-sized (same format/coords as the persistent target), so
    /// scene pipelines, scissors and verts work unchanged inside a layer;
    /// texture-bound so EndLayer can sample them as blur/composite sources.
    /// Grown on demand, dropped on resize (sizes stale).
    layer_pool: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// The two ping-pong targets of the separable blur (H then V). Two
    /// suffice at any nesting depth: an EndLayer runs its blur passes to
    /// completion before the enclosing layer continues.
    blur_ping: Option<((wgpu::Texture, wgpu::TextureView), (wgpu::Texture, wgpu::TextureView))>,
    /// Liquid-glass backdrop pool, one slot per open glass layer (slot =
    /// layer depth): the parent snapshot taken at EndLayer (before any
    /// composite writes), its single core-blurred copy, and the optics
    /// output. Surface-sized, grown on demand, dropped on resize.
    glass_pool: Vec<GlassTargets>,
    /// Source-keyed cache of custom (material-DSL) stage pipelines
    /// (D-M5: keyed on the generated WGSL source alone, a compile-time
    /// constant — never on uniform values). `None` value = the source
    /// failed naga validation / pipeline creation once; remembered so a
    /// malformed surface drops its stage loudly instead of retrying
    /// every frame. Grown on demand (a structurally new material compiles
    /// its pipeline on first sight).
    custom_pipelines: std::collections::HashMap<u64, Option<std::sync::Arc<wgpu::RenderPipeline>>>,
    /// G1 proof counters (D-M5): `custom_cache_builds` counts sync pipeline
    /// builds (first sight of a source); `custom_cache_hits` counts
    /// immutable cache reads in the walk. A hover repaint must move hits
    /// only — builds stay flat (same source, new uniform values). `Cell`
    /// because the walk reader holds `&self`.
    custom_cache_hits: std::cell::Cell<u64>,
    custom_cache_builds: std::cell::Cell<u64>,
    /// Parent snapshot for blend-mode alpha shadows (one surface-sized
    /// slot, reused across EndLayers): copied from the parent target
    /// after the spill and before the shadow draws, so a shadow
    /// multiplies against the finished beneath — the same isolation
    /// the layer blend path gets at BeginLayer, but lazy (the stash is
    /// only known once the layer's content has emitted). Dropped on
    /// resize.
    shadow_snap: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Device-px regions of the PREVIOUS frame's root offscreen layers.
    /// A partial-damage frame Loads the persistent target, preserving
    /// the last frame's layer composite wherever damage didn't cover;
    /// a glass layer's BeginLayer snapshot would then capture its own
    /// previous output and the optics would re-apply to it every frame
    /// (progressive white-out). The damage-base repaints these regions
    /// to the base color at the start of the next frame so the snapshot
    /// always sees fresh parent content. Collected during the op walk;
    /// consumed (and replaced) at the next frame's start.
    stale_root_layers: Vec<ResolvedRect>,
}

fn make_ortho(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    w: u32,
    h: u32,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    use wgpu::util::DeviceExt;
    let data = [w.max(1) as f32, h.max(1) as f32, 0.0, 0.0];
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("riode-wgpu-ortho"),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("riode-wgpu-ortho-bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bg)
}

fn make_vert_buf(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.max(256),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// One filter-layer offscreen slot: surface-sized Rgba8Unorm, same
/// format/coords as the persistent target so scene passes, scissors and
/// verts work unchanged inside the layer; texture-bound so the layer can
/// be sampled as a blur/composite source. COPY_SRC lets a nested glass
/// layer snapshot this layer as its backdrop. (Unlike `make_persistent`
/// it needs no readback — nothing maps it.)
fn make_filter_target(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Parent snapshot for blend-mode alpha shadows (see `shadow_snap`):
/// copy destination + sampled backdrop — never a render attachment.
fn make_shadow_snap(
    device: &wgpu::Device,
    w: u32,
    h: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("riode-wgpu-shadow-snap"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// One liquid-glass backdrop slot (surface-sized): the parent snapshot
/// taken at BeginLayer (after the parent pass ends, before this layer's
/// children draw), the single core-blurred copy the optics pass samples
/// (one blur at the composed sigma — wide tail, post-lens blur and
/// per-shape extra frost are shader taps on it, design decision 3), and
/// the optics output (composite source, and shadow silhouette for
/// non-glass paths). The snapshot needs COPY_DST (filled by
/// `copy_texture_to_texture`); a/out need RENDER_ATTACHMENT
/// (blur/optics writes) and TEXTURE_BINDING (glass reads / composite
/// samples).
struct GlassTargets {
    snap: (wgpu::Texture, wgpu::TextureView),
    a: (wgpu::Texture, wgpu::TextureView),
    out: (wgpu::Texture, wgpu::TextureView),
}

fn make_glass_targets(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    depth: usize,
) -> GlassTargets {
    fn make(
        device: &wgpu::Device,
        w: u32,
        h: u32,
        label: String,
        usage: wgpu::TextureUsages,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&label),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
    let rw = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    GlassTargets {
        snap: make(
            device,
            w,
            h,
            format!("riode-wgpu-glass-snap-{depth}"),
            wgpu::TextureUsages::COPY_DST | rw,
        ),
        a: make(device, w, h, format!("riode-wgpu-glass-a-{depth}"), rw),
        out: make(device, w, h, format!("riode-wgpu-glass-out-{depth}"), rw),
    }
}

/// One open filter layer at draw time: its pool slot plus the
/// emission-time region and filter-params index (`filter_params` is
/// frame-local; layer chunks never replay from cache), the optional
/// glass-params index, and the ordered effect stages to execute.
struct OpenLayer {
    slot: usize,
    region: ResolvedRect,
    params: u32,
    glass: Option<u32>,
    stages: Vec<StageGpu>,
    backdrop_stages: Vec<StageGpu>,
}

/// Where the executor's current layer image lives: the layer offscreen
/// or one scratch target of the blur ping-pong pair.
#[derive(Clone, Copy, PartialEq)]
enum CurLoc {
    Layer,
    Ping0,
    Ping1,
}

/// Where the backdrop walk's current image lives: the raw parent
/// snapshot, one scratch target, or the glass pre-blur input (the
/// walk's final destination — the pre-blur and blend stages read the
/// effected backdrop from there).
#[derive(Clone, Copy, PartialEq)]
enum BdLoc {
    Snap,
    Ping0,
    Ping1,
    GlassA,
}

/// Where the lens card's image lives during trailing backdrop
/// transforms: the glass out texture or one scratch ping.
#[derive(Clone, Copy, PartialEq)]
enum CardLoc {
    GtOut,
    Ping0,
    Ping1,
}

impl WgpuRenderer {
    pub fn new(gpu: WgpuContext, text_ctx: TextContext) -> Self {
        let pipes = create_pipelines(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            gpu.config.format,
        );
        let (ortho_buf, ortho_bg) = make_ortho(
            &gpu.device,
            &pipes.ortho_layout,
            gpu.config.width,
            gpu.config.height,
        );
        let atlas = GlyphAtlas::new(&gpu.device, &text_ctx);
        let glyph_bg_linear = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-glyph-bg-linear"),
            layout: &pipes.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipes.glyph_sampler_linear),
                },
            ],
        });
        let glyph_bg_nearest = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-glyph-bg-nearest"),
            layout: &pipes.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipes.glyph_sampler_nearest),
                },
            ],
        });
        let vert_cap = 4096u64;
        let vert_buf = make_vert_buf(
            &gpu.device,
            "riode-wgpu-verts",
            vert_cap * std::mem::size_of::<RectVertex>() as u64,
        );
        let glyph_cap = 4096u64;
        let glyph_buf = make_vert_buf(
            &gpu.device,
            "riode-wgpu-glyphs",
            glyph_cap * std::mem::size_of::<GlyphVertex>() as u64,
        );
        let rrect_cap = 1024u64;
        let rrect_buf = make_vert_buf(
            &gpu.device,
            "riode-wgpu-rrects",
            rrect_cap * std::mem::size_of::<RRectVertex>() as u64,
        );
        let shadow_cap = 512u64;
        let shadow_buf = make_vert_buf(
            &gpu.device,
            "riode-wgpu-shadows",
            shadow_cap * std::mem::size_of::<ShadowVertex>() as u64,
        );
        let image_cap = 512u64;
        let image_buf = make_vert_buf(
            &gpu.device,
            "riode-wgpu-images",
            image_cap * std::mem::size_of::<ImageVertex>() as u64,
        );
        let images = ImageCache::new(&gpu.device);
        let svgs = SvgCache::new();
        Self {
            gpu,
            pipes,
            batch: Batch::new(),
            atlas,
            text_ctx,
            ortho_buf,
            ortho_bg,
            glyph_bg_linear,
            glyph_bg_nearest,
            vert_buf,
            vert_cap,
            glyph_buf,
            glyph_cap,
            rrect_buf,
            rrect_cap,
            shadow_buf,
            shadow_cap,
            image_buf,
            image_cap,
            images,
            svgs,
            scale: 1.0,
            chunks: ChunkCache::default(),
            clear_next: true,
            readback: None,
            layer_pool: Vec::new(),
            blur_ping: None,
            shadow_snap: None,
            glass_pool: Vec::new(),
            custom_pipelines: std::collections::HashMap::new(),
            custom_cache_hits: std::cell::Cell::new(0),
            custom_cache_builds: std::cell::Cell::new(0),
            stale_root_layers: Vec::new(),
        }
    }

    /// Sets the layout→device scale (window scale factor). Takes effect
    /// next frame via the batch root transform.
    pub fn set_scale(&mut self, dsf: f32) {
        let dsf = if dsf.is_finite() {
            dsf.clamp(0.5, 4.0)
        } else {
            1.0
        };
        if (dsf - self.scale).abs() > 1e-4 {
            tracing::info!(scale = dsf, "riode-wgpu device scale changed");
        }
        self.scale = dsf;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (ow, oh) = (self.gpu.config.width, self.gpu.config.height);
        self.gpu.resize(width, height);
        if ow != self.gpu.config.width || oh != self.gpu.config.height {
            let (buf, bg) = make_ortho(
                &self.gpu.device,
                &self.pipes.ortho_layout,
                self.gpu.config.width,
                self.gpu.config.height,
            );
            self.ortho_buf = buf;
            self.ortho_bg = bg;
            // Fresh target memory is undefined: the next frame must Clear
            // rather than Load, or garbage persists in undrawn pixels.
            self.clear_next = true;
            // Cached verts are baked in device px of the old size: drop
            // them so every chunk re-emits at the new size. (Atlas
            // coverage is size-independent and survives.)
            self.chunks.clear();
            // Filter-layer targets are surface-sized: stale sizes are
            // unusable, and the next frame recreates slots on demand.
            self.layer_pool.clear();
            self.blur_ping = None;
            self.glass_pool.clear();
            self.shadow_snap = None;
            self.stale_root_layers.clear();
        }
    }

    fn ensure_rect_cap(&mut self, need: u64) {
        if need <= self.vert_cap {
            return;
        }
        let mut cap = self.vert_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.vert_buf = make_vert_buf(
            &self.gpu.device,
            "riode-wgpu-verts",
            cap * std::mem::size_of::<RectVertex>() as u64,
        );
        self.vert_cap = cap;
    }

    fn ensure_glyph_cap(&mut self, need: u64) {
        if need <= self.glyph_cap {
            return;
        }
        let mut cap = self.glyph_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.glyph_buf = make_vert_buf(
            &self.gpu.device,
            "riode-wgpu-glyphs",
            cap * std::mem::size_of::<GlyphVertex>() as u64,
        );
        self.glyph_cap = cap;
    }

    fn ensure_rrect_cap(&mut self, need: u64) {
        if need <= self.rrect_cap {
            return;
        }
        let mut cap = self.rrect_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.rrect_buf = make_vert_buf(
            &self.gpu.device,
            "riode-wgpu-rrects",
            cap * std::mem::size_of::<RRectVertex>() as u64,
        );
        self.rrect_cap = cap;
    }

    fn ensure_shadow_cap(&mut self, need: u64) {
        if need <= self.shadow_cap {
            return;
        }
        let mut cap = self.shadow_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.shadow_buf = make_vert_buf(
            &self.gpu.device,
            "riode-wgpu-shadows",
            cap * std::mem::size_of::<ShadowVertex>() as u64,
        );
        self.shadow_cap = cap;
    }

    fn ensure_image_cap(&mut self, need: u64) {
        if need <= self.image_cap {
            return;
        }
        let mut cap = self.image_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.image_buf = make_vert_buf(
            &self.gpu.device,
            "riode-wgpu-images",
            cap * std::mem::size_of::<ImageVertex>() as u64,
        );
        self.image_cap = cap;
    }

    /// Bounds culling viewport in layout px: chunk bounds and damage are
    /// layout-space, while the swapchain config is device px.
    fn viewport(&self) -> ResolvedRect {
        let s = self.scale.max(0.5);
        ResolvedRect::new(
            0.0,
            0.0,
            self.gpu.config.width as f32 / s,
            self.gpu.config.height as f32 / s,
        )
    }

    /// Whether the persistent scene target must be cleared this frame.
    ///
    /// The target retains the previous frame, so a partial damage must
    /// LOAD to preserve undamaged regions (the damaged area is repainted
    /// from base by damage-base quads first). Clearing is correct only
    /// when every visible chunk repaints: empty damage (the backend only
    /// reaches here with `needs_present`, i.e. a full redraw) or right
    /// after the target was (re)created, whose memory is undefined.
    /// Clearing on any other damage punches holes everywhere damage
    /// doesn't cover, which later Load frames then preserve indefinitely.
    fn must_clear(&mut self, damage: &DamageRegion) -> bool {
        if self.clear_next {
            self.clear_next = false;
            return true;
        }
        damage.is_empty()
    }

    /// Device-px scissor rect for an optional clip/bounds rect.
    /// Rounds (not truncates) so fractional rects snap symmetrically;
    /// returns None when empty or fully outside the surface.
    fn scissor_rect(
        scissor: &Option<ResolvedRect>,
        sw: u32,
        sh: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        match scissor {
            Some(r) => {
                let x = r.x.max(0.0).round() as u32;
                let y = r.y.max(0.0).round() as u32;
                let xe = (r.x + r.width).max(0.0).round() as u32;
                let ye = (r.y + r.height).max(0.0).round() as u32;
                let w = xe.saturating_sub(x).min(sw.saturating_sub(x.min(sw)));
                let h = ye.saturating_sub(y).min(sh.saturating_sub(y.min(sh)));
                if w == 0 || h == 0 {
                    return None;
                }
                Some((x, y, w, h))
            }
            None => Some((0, 0, sw, sh)),
        }
    }

    fn apply_scissor(
        pass: &mut wgpu::RenderPass<'_>,
        scissor: &Option<ResolvedRect>,
        sw: u32,
        sh: u32,
    ) -> bool {
        match Self::scissor_rect(scissor, sw, sh) {
            Some((x, y, w, h)) => {
                pass.set_scissor_rect(x, y, w, h);
                true
            }
            None => false,
        }
    }

    /// One separable-blur pass over a filter-layer source: fullscreen
    /// triangle, nine textureLoad taps along `dir`, replace blend into
    /// `dst` (cleared by its load op), scissored to `sc` — which the
    /// caller already resolved (a region that misses the surface is
    /// skipped entirely, CPU-side).
    fn run_blur_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        sigma: f32,
        dir: (f32, f32),
        sc: (u32, u32, u32, u32),
    ) {
        // Params + bind group live only for this encoder pass: wgpu
        // refcounts the underlying resources into the bind group, and
        // drop order (pass, then bg, then buffer — declaration order
        // below) keeps every reference valid.
        let ubuf = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riode-wgpu-blur-params"),
            contents: bytemuck::bytes_of(&BlurGpu {
                sigma,
                dir_x: dir.0,
                dir_y: dir.1,
                pad: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("riode-wgpu-blur-bg"),
                layout: &self.pipes.filter_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: ubuf.as_entire_binding(),
                    },
                ],
            });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("riode-wgpu-blur-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_scissor_rect(sc.0, sc.1, sc.2, sc.3);
        pass.set_pipeline(&self.pipes.blur_pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..6, 0..1);
    }

    /// One liquid-glass optics pass: fullscreen triangle over the
    /// core-blurred backdrop, into `dst` (cleared by its load op),
    /// scissored to the layer region. `sharp` is the raw parent
    /// snapshot — binding 3, the card-blend backdrop (`backdrop` may be
    /// the core pre-blur). Params + bind group live only for this
    /// encoder pass (refcount pattern of `run_blur_pass`).
    fn run_glass_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        backdrop: &wgpu::TextureView,
        sharp: &wgpu::TextureView,
        mut gp: GlassFrameGpu,
        dst: &wgpu::TextureView,
        sc: (u32, u32, u32, u32),
    ) {
        // Engine-owned header: the shader's `uSize` is the backdrop target
        // size, known only here (emission packs without it).
        gp.size = [
            self.gpu.config.width as f32,
            self.gpu.config.height as f32,
            0.0,
            0.0,
        ];
        let ubuf = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riode-wgpu-glass-params"),
            contents: bytemuck::bytes_of(&gp),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("riode-wgpu-glass-bg"),
                layout: &self.pipes.glass_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.pipes.glass_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: ubuf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(sharp),
                    },
                ],
            });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("riode-wgpu-glass-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_scissor_rect(sc.0, sc.1, sc.2, sc.3);
        pass.set_pipeline(&self.pipes.glass_pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..6, 0..1);
    }

    /// Get-or-create the render pipeline for a custom (material-DSL)
    /// stage's WGSL source, keyed by an FNV-1a hash of the source alone
    /// (D-M5: the pipeline cache is source-keyed — a compile-time
    /// constant — never on uniform values, so a hover repaint reuses the
    /// same pipeline). Composes the complete module exactly like the wire
    /// does — `EFFECT_ABI_WGSL` + the custom `apply` + the engine entry
    /// scaffold — validates it with naga (the synchronous twin of
    /// `riode-shader-glue`'s transpile), and returns `None` (remembered)
    /// on any failure so a malformed surface drops its stage loudly
    /// instead of tripping a wgpu uncaptured-error panic.
    fn custom_pipeline_for(&mut self, source: &str) -> Option<std::sync::Arc<wgpu::RenderPipeline>> {
        use riode::foundation::effect::shaders::{EFFECT_ABI_WGSL, ENGINE_ENTRY_WGSL};
        let mut h: u64 = 0xcbf29ce484222325;
        for b in source.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        if let Some(cached) = self.custom_pipelines.get(&h) {
            self.custom_cache_hits.set(self.custom_cache_hits.get() + 1);
            return cached.clone();
        }
        let full = format!("{EFFECT_ABI_WGSL}\n{source}\n{ENGINE_ENTRY_WGSL}");
        let built = Self::build_custom_pipeline(&self.gpu.device, &self.pipes, &full);
        if let Err(reason) = &built {
            tracing::warn!("riode-wgpu: custom stage dropped ({reason})");
        }
        self.custom_cache_builds.set(self.custom_cache_builds.get() + 1);
        let entry = built.ok().map(std::sync::Arc::new);
        self.custom_pipelines.insert(h, entry.clone());
        entry
    }

    /// Immutable cache read for the in-walk executor arm: the pre-warm
    /// above the op loop already inserted every custom source (`Some` =
    /// ready pipeline, `None` = failed build, warned once), so the walk
    /// — which holds `&self.batch` — never needs `&mut self` here.
    /// A successful read bumps the G1 hit counter (builds stay flat on
    /// hover repaints — same source, new uniform values).
    fn custom_pipeline_cached(
        &self,
        source: &str,
    ) -> Option<std::sync::Arc<wgpu::RenderPipeline>> {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in source.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let hit = self.custom_pipelines.get(&h).cloned().flatten();
        if hit.is_some() {
            self.custom_cache_hits.set(self.custom_cache_hits.get() + 1);
        }
        hit
    }

    /// Startup pre-warm entry (D-M6): compile every custom source in the
    /// app-level const manifest (`MATERIAL_SURFACE_SOURCES`) before frame
    /// 1, so first paint never pays a sync build. Idempotent — already
    /// cached sources count as hits, not builds. Settled form: app-level
    /// manifest, not linkme (no new link-section dependency; the macro
    /// cannot cross crate boundaries anyway).
    pub fn prewarm_material_sources(&mut self, sources: &[&str]) {
        for src in sources {
            self.custom_pipeline_for(src);
        }
    }

    /// G1 proof counters: `(hits, builds)`. After first paint, hovering
    /// the sheen row must move hits only.
    pub fn custom_cache_stats(&self) -> (u64, u64) {
        (self.custom_cache_hits.get(), self.custom_cache_builds.get())
    }

    /// Validate + build one custom-stage pipeline from a complete WGSL
    /// module (ABI + `apply` + entry). Returns the pipeline or a
    /// human-readable reason, mirroring the shader-glue's error contract.
    fn build_custom_pipeline(
        device: &wgpu::Device,
        pipes: &Pipelines,
        full: &str,
    ) -> Result<wgpu::RenderPipeline, String> {
        // naga first: a clean, synchronous rejection reason.
        let module = naga::front::wgsl::parse_str(full).map_err(|e| format!("wgsl parse: {e}"))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| format!("wgsl validate: {e}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("riode-wgpu-custom"),
            source: wgpu::ShaderSource::Wgsl(full.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("riode-wgpu-custom-pipe-layout"),
            bind_group_layouts: &[&pipes.custom_layout],
            push_constant_ranges: &[],
        });
        Ok(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("riode-wgpu-custom-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // The card / glass-pool targets are Rgba8Unorm
                    // (`make_glass_targets`), so the custom stage writes
                    // the same format the merge-under-sheet reads.
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        }))
    }

    /// One custom (material-DSL) stage pass: a fullscreen triangle over
    /// `dst` (scissor-clipped), binding {0 sampler, 1 uniforms, 2 input,
    /// 3 backdrop, 4 sharp} exactly as the engine entry scaffold declares
    /// them. The scaffold seeds `ctx.color` from `input` (the card), calls
    /// the material's `apply`, and the result lands in `dst`.
    #[allow(clippy::too_many_arguments)]
    fn run_custom_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        pipe: &wgpu::RenderPipeline,
        input: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        sharp: &wgpu::TextureView,
        uniforms: &[f32],
        dst: &wgpu::TextureView,
        sc: (u32, u32, u32, u32),
    ) {
        // Uniform upload sized to the ABI's fixed `CustomUniforms`
        // (`array<vec4<f32>, 8>` = 32 f32 = 128 bytes). wgpu enforces the
        // buffer is at least the struct's min size, so always fill 32.
        // Engine-side size is otherwise free (D-M7) — a generated sized
        // struct (Phase 2+) replaces both this cap and the scaffold's
        // fixed array together.
        let mut padded = uniforms.to_vec();
        padded.resize(32, 0.0);
        let bytes: Vec<u8> = padded.iter().flat_map(|f| f.to_le_bytes()).collect();
        let ubuf = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riode-wgpu-custom-params"),
            contents: &bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-custom-bg"),
            layout: &self.pipes.custom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.pipes.custom_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: ubuf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(sharp),
                },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("riode-wgpu-custom-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_scissor_rect(sc.0, sc.1, sc.2, sc.3);
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..6, 0..1);
    }

    /// One composite draw of a layer-sized quad (`verts` in `rect_verts`,
    /// position-derived sampling) through `matrix` into `dst`.
    ///
    /// `clear = true` starts from transparent (intermediate stage
    /// targets); src-over onto transparent is identity, so the plain
    /// composite pipeline doubles as the stage applicator. `clear =
    /// false` layers over `dst`'s current pixels (glass-merge second
    /// draw). The material is pushed into `kept` and must outlive the
    /// returned-then-dropped pass — the caller keeps `kept` alive.
    #[allow(clippy::too_many_arguments)]
    fn run_composite_quad(
        &self,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        matrix: &ColorMatrixGpu,
        dst: &wgpu::TextureView,
        verts: std::ops::Range<u32>,
        sc: (u32, u32, u32, u32),
        clear: bool,
        kept: &mut Vec<wgpu::BindGroup>,
    ) {
        let sbuf = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riode-wgpu-stage-matrix"),
            contents: bytemuck::bytes_of(matrix),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        kept.push(self.gpu.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("riode-wgpu-stage-material"),
                layout: &self.pipes.filter_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: sbuf.as_entire_binding(),
                    },
                ],
            },
        ));
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("riode-wgpu-stage-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_scissor_rect(sc.0, sc.1, sc.2, sc.3);
        pass.set_pipeline(&self.pipes.composite_pipeline);
        pass.set_bind_group(0, &self.ortho_bg, &[]);
        pass.set_vertex_buffer(0, self.vert_buf.slice(..));
        pass.set_bind_group(1, kept.last().unwrap(), &[]);
        pass.draw(verts, 0..1);
    }

    /// One blend-composite draw (`blend_layout`: source + parent snapshot
    /// + mode in `tex_off.z`) into `dst`, cleared or layered like
    /// [`Self::run_composite_quad`]. Used for mid-stack `Blend` stages
    /// (materialize) and the pending-blend final composite.
    #[allow(clippy::too_many_arguments)]

    /// Texture view for a [`BdLoc`]: the raw parent snapshot, one
    /// scratch target, or the glass pre-blur input (also the walk's
    /// final destination).
    fn bd_view(&self, slot: usize, loc: BdLoc) -> &wgpu::TextureView {
        match loc {
            BdLoc::Snap => &self.glass_pool[slot].snap.1,
            BdLoc::Ping0 => &self.blur_ping.as_ref().expect("ping pair allocated").0 .1,
            BdLoc::Ping1 => &self.blur_ping.as_ref().expect("ping pair allocated").1 .1,
            BdLoc::GlassA => &self.glass_pool[slot].a.1,
        }
    }

    /// Scratch destination for one backdrop stage: the other ping when
    /// the source is a ping, `Ping0` from the snapshot, and the pre-blur
    /// input for the final stage.
    fn bd_dest(cur: BdLoc, last: bool) -> BdLoc {
        if last {
            return BdLoc::GlassA;
        }
        match cur {
            BdLoc::Snap | BdLoc::Ping1 => BdLoc::Ping0,
            BdLoc::Ping0 | BdLoc::GlassA => BdLoc::Ping1,
        }
    }

    /// Texture view for a [`CardLoc`]: the glass out texture or one
    /// scratch ping.
    fn card_view(&self, slot: usize, loc: CardLoc) -> &wgpu::TextureView {
        match loc {
            CardLoc::GtOut => &self.glass_pool[slot].out.1,
            CardLoc::Ping0 => &self.blur_ping.as_ref().expect("ping pair allocated").0 .1,
            CardLoc::Ping1 => &self.blur_ping.as_ref().expect("ping pair allocated").1 .1,
        }
    }

    /// Scratch destination for one trailing card stage: the other ping
    /// (the card's own location may be the out texture or either ping).
    fn card_scratch(loc: CardLoc) -> CardLoc {
        match loc {
            CardLoc::Ping0 => CardLoc::Ping1,
            _ => CardLoc::Ping0,
        }
    }

    /// The scene pass for a content op: markers manage pass lifetimes
    /// themselves (they re-establish one before the next content op), so
    /// an open pass is an invariant here.
    fn open_pass<'a, 'b>(p: &'a mut Option<wgpu::RenderPass<'b>>) -> &'a mut wgpu::RenderPass<'b> {
        p.as_mut()
            .expect("riode-wgpu: scene pass must be open at content op")
    }

    /// Texture view for a [`CurLoc`]: the layer offscreen or one scratch
    /// target. Panics when a scratch is addressed without an allocated
    /// ping pair — the EndLayer arm allocates whenever any stage (or
    /// shadow, or glass) can need one.
    fn stage_view(&self, slot: usize, loc: CurLoc) -> &wgpu::TextureView {
        match loc {
            CurLoc::Layer => &self.layer_pool[slot].1,
            CurLoc::Ping0 => &self.blur_ping.as_ref().expect("ping pair allocated").0 .1,
            CurLoc::Ping1 => &self.blur_ping.as_ref().expect("ping pair allocated").1 .1,
        }
    }

    /// Two scratch locs for a separable blur, both distinct from `cur`
    /// (H reads `cur`, V reads the H output — neither may alias its
    /// source). Also serves the shadow blur and the spill iterations.
    fn stage_pair(cur: CurLoc) -> (CurLoc, CurLoc) {
        match cur {
            CurLoc::Layer => (CurLoc::Ping0, CurLoc::Ping1),
            CurLoc::Ping0 => (CurLoc::Ping1, CurLoc::Layer),
            CurLoc::Ping1 => (CurLoc::Ping0, CurLoc::Layer),
        }
    }

    /// One scratch loc distinct from `cur`, for single-pass stages
    /// (color matrix, glass merge, blend materialize).
    fn stage_scratch(cur: CurLoc) -> CurLoc {
        match cur {
            CurLoc::Layer | CurLoc::Ping1 => CurLoc::Ping0,
            CurLoc::Ping0 => CurLoc::Ping1,
        }
    }

    /// Shades the scene into the persistent target without presenting.
    ///
    /// Same batching and scene pass as [`Self::render_frame`] minus the
    /// swapchain acquire, blit and present: the quad-combine backend reads
    /// the persistent texture back to CPU (see [`Self::readback_rgba8`]).
    pub fn render_scene_offscreen(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let viewport = self.viewport();
        // Resolve the one-shot clear flag before the per-field borrows
        // below (must_clear takes &mut self).
        let clear = self.must_clear(damage);
        // Previous frame's root offscreen-layer regions: repainted to
        // base below so a Load frame never preserves a layer's own
        // composite under its next snapshot (the glass feedback loop).
        let stale_layers = std::mem::take(&mut self.stale_root_layers);

        // Batch scene content: viewport cull + damage cull, then per-chunk
        // vertex cache replay (static chunks skip command re-emit and
        // glyph re-raster entirely) or fresh emission on miss.
        // Disjoint field borrows keep borrowck happy.
        {
            let (batch, atlas, text_ctx, gpu, chunks, images, svgs) = (
                &mut self.batch,
                &mut self.atlas,
                &self.text_ctx,
                &self.gpu,
                &mut self.chunks,
                &mut self.images,
                &mut self.svgs,
            );
            atlas.next_frame();
            chunks.next_frame();
            images.next_frame();
            svgs.next_frame();
            batch.set_scale(self.scale);
            batch.reset();
            // Tile semantics for partial damage: repaint the damaged area
            // from base color first (opaque base quads), so vacated pixels
            // (removed/moved nodes) don't ghost under the Load below.
            // Decided here so the quads precede all chunk content.
            // `clear` was resolved before the per-field borrows below.
            if !clear {
                batch.emit_damage_base(damage, self.scale, base_color);
                // Same tile rule for last frame's root layer composites:
                // they re-render every frame, so their region is
                // implicitly damaged — repaint it to base before any
                // parent draw, or the layer's BeginLayer snapshot
                // captures its own previous output.
                for r in &stale_layers {
                    batch.emit_base_rect_dev(r, base_color);
                }
            }
            let sq16 = (riode::text::quantize_raster_scale(self.scale) * 16.0).round() as u16;
            let agen = atlas.generation();
            for chunk in scene.chunks.iter() {
                // Viewport cull only. There is deliberately NO damage
                // cull here: repainting a lower chunk without repainting
                // the chunks above it would bury their stale pixels
                // wherever they overlap on screen (draws are full
                // geometry, not damage-scissored). Like vello, every
                // visible chunk repaints every presented frame, so
                // painter's order is correct by construction; damage
                // still drives present-skip and Load/Clear decisions.
                if !chunk.bounds.overlaps(&viewport) {
                    continue;
                }
                let key = chunk_key(chunk, sq16);
                // Chunk boundary: runs must not merge across chunks, or the
                // per-chunk snapshot below would slice verts whose draw ops
                // belong to the previous chunk (invisible on cache replay).
                batch.begin_chunk();
                // Origin-rebased replay: a hit at a new origin translates
                // cached verts instead of re-tessellating (scroll); a hit at
                // the baked origin is a plain copy. Anything rebase_for
                // rejects (scale change, local frames, fractional glyph
                // phase) falls through to fresh emission below.
                let replayed = match chunks.get(key, agen) {
                    Some(hit) => match hit.rebase_for(chunk.abs_origin, batch.scale()) {
                        Some((0.0, 0.0)) => {
                            let rb = batch.rect_verts.len() as u32;
                            let gb = batch.glyph_verts.len() as u32;
                            let qb = batch.rrect_verts.len() as u32;
                            let sb = batch.shadow_verts.len() as u32;
                            let ib = batch.image_verts.len() as u32;
                            let ibb = batch.image_binds.len() as u32;
                            batch.rect_verts.extend_from_slice(&hit.rects);
                            batch.glyph_verts.extend_from_slice(&hit.glyphs);
                            batch.rrect_verts.extend_from_slice(&hit.rrects);
                            batch.shadow_verts.extend_from_slice(&hit.shadows);
                            batch.image_verts.extend_from_slice(&hit.images);
                            batch
                                .image_binds
                                .extend(hit.image_binds.iter().map(Arc::clone));
                            // Extend in place (N2): the temp Vec was a per-chunk
                            // alloc on the replay path.
                            batch.ops.extend(
                                hit.ops
                                    .iter()
                                    .map(|op| ChunkCache::rebased(op, rb, gb, qb, sb, ib, ibb)),
                            );
                            true
                        }
                        Some((dx, dy)) => {
                            let rb = batch.rect_verts.len() as u32;
                            let gb = batch.glyph_verts.len() as u32;
                            let qb = batch.rrect_verts.len() as u32;
                            let sb = batch.shadow_verts.len() as u32;
                            let ib = batch.image_verts.len() as u32;
                            let ibb = batch.image_binds.len() as u32;
                            hit.append_rebased(
                                dx,
                                dy,
                                &mut batch.rect_verts,
                                &mut batch.glyph_verts,
                                &mut batch.rrect_verts,
                                &mut batch.shadow_verts,
                                &mut batch.image_verts,
                            );
                            batch
                                .image_binds
                                .extend(hit.image_binds.iter().map(Arc::clone));
                            // Extend in place (N2): the temp Vec was a per-chunk
                            // alloc on the replay path.
                            batch.ops.extend(
                                hit.ops
                                    .iter()
                                    .map(|op| ChunkCache::rebased(op, rb, gb, qb, sb, ib, ibb)),
                            );
                            true
                        }
                        None => false,
                    },
                    None => false,
                };
                if replayed {
                    continue;
                }
                let (r0, g0, q0, s0, i0, ib0, o0) = (
                    batch.rect_verts.len(),
                    batch.glyph_verts.len(),
                    batch.rrect_verts.len(),
                    batch.shadow_verts.len(),
                    batch.image_verts.len(),
                    batch.image_binds.len(),
                    batch.ops.len(),
                );
                for clip in chunk.inherited_clips.iter() {
                    batch.push_clip_device(&clip.rect, clip.radius);
                }
                batch.emit_chunk(
                    &riode::scene::chunk_base(chunk),
                    &chunk.commands,
                    text_ctx,
                    atlas,
                    &gpu.queue,
                    &gpu.device,
                    images,
                    svgs,
                );
                for _ in chunk.inherited_clips.iter() {
                    batch.clip.pop();
                }
                // Snapshot the new ranges with buffer-relative op starts.
                let r0u = r0 as u32;
                let g0u = g0 as u32;
                let q0u = q0 as u32;
                let s0u = s0 as u32;
                let i0u = i0 as u32;
                let ib0u = ib0 as u32;
                let rects = batch.rect_verts[r0..].to_vec();
                let glyphs = batch.glyph_verts[g0..].to_vec();
                let rrects = batch.rrect_verts[q0..].to_vec();
                let shadows = batch.shadow_verts[s0..].to_vec();
                let images_v = batch.image_verts[i0..].to_vec();
                let image_binds = batch.image_binds[ib0..].to_vec();
                let ops: Vec<Op> = batch.ops[o0..]
                    .iter()
                    .map(|op| match op.clone() {
                        // BaseQuads are emitted before the chunk loop, so a
                        // chunk snapshot can never contain one; pass through
                        // defensively (never replayed from cache).
                        Op::BaseQuads { start, count } => Op::BaseQuads {
                            start: start - r0u,
                            count,
                        },
                        Op::RRects {
                            start,
                            count,
                            scissor,
                        } => Op::RRects {
                            start: start - q0u,
                            count,
                            scissor,
                        },
                        Op::Rects {
                            start,
                            count,
                            scissor,
                        } => Op::Rects {
                            start: start - r0u,
                            count,
                            scissor,
                        },
                        Op::Glyphs {
                            start,
                            count,
                            scissor,
                            linear,
                        } => Op::Glyphs {
                            start: start - g0u,
                            count,
                            scissor,
                            linear,
                        },
                        Op::Shadows {
                            start,
                            count,
                            scissor,
                        } => Op::Shadows {
                            start: start - s0u,
                            count,
                            scissor,
                        },
                        Op::Images {
                            start,
                            count,
                            scissor,
                            bind,
                        } => Op::Images {
                            start: start - i0u,
                            count,
                            scissor,
                            bind: bind - ib0u,
                        },
                        // Filter layers re-emit fresh every frame (the
                        // stream is non-rebaseable, see cache.rs); the
                        // arms keep the snapshot self-consistent anyway.
                        Op::BeginLayer {
                            region,
                            params,
                            glass,
                            stages,
                            backdrop_stages,
                        } => Op::BeginLayer {
                            region,
                            params,
                            glass,
                            stages,
                            backdrop_stages,
                        },
                        Op::EndLayer {
                            start,
                            count,
                            scissor,
                            shadow_start,
                            shadows,
                            spill_start,
                            spill_count,
                        } => Op::EndLayer {
                            start: start - r0u,
                            count,
                            scissor,
                            shadow_start: shadow_start - r0u,
                            shadows,
                            spill_start: spill_start - r0u,
                            spill_count,
                        },
                    })
                    .collect();
                chunks.insert(
                    key,
                    crate::cache::CachedChunk::new(
                        rects,
                        glyphs,
                        rrects,
                        shadows,
                        images_v,
                        image_binds,
                        ops,
                        atlas.generation(),
                        chunk.abs_origin,
                        batch.scale(),
                        crate::cache::rebaseable_stream(&chunk.commands),
                    ),
                );
            }
        }

        // Upload verts + ortho before opening the pass.
        if !self.batch.rect_verts.is_empty() {
            self.ensure_rect_cap(self.batch.rect_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.vert_buf,
                0,
                bytemuck::cast_slice(&self.batch.rect_verts),
            );
        }
        if !self.batch.glyph_verts.is_empty() {
            self.ensure_glyph_cap(self.batch.glyph_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.glyph_buf,
                0,
                bytemuck::cast_slice(&self.batch.glyph_verts),
            );
        }
        if !self.batch.rrect_verts.is_empty() {
            self.ensure_rrect_cap(self.batch.rrect_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.rrect_buf,
                0,
                bytemuck::cast_slice(&self.batch.rrect_verts),
            );
        }
        if !self.batch.shadow_verts.is_empty() {
            self.ensure_shadow_cap(self.batch.shadow_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.shadow_buf,
                0,
                bytemuck::cast_slice(&self.batch.shadow_verts),
            );
        }
        if !self.batch.image_verts.is_empty() {
            self.ensure_image_cap(self.batch.image_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.image_buf,
                0,
                bytemuck::cast_slice(&self.batch.image_verts),
            );
        }
        {
            let wh = [
                self.gpu.config.width as f32,
                self.gpu.config.height as f32,
                0.0,
                0.0,
            ];
            self.gpu
                .queue
                .write_buffer(&self.ortho_buf, 0, bytemuck::cast_slice(&wh));
        }

        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("riode-wgpu-frame"),
            });

        // Scene pass into persistent target, ops in command order.
        // Partial damage Loads (preserves undamaged regions, with the
        // damaged area pre-painted to base above); empty damage or a fresh
        // target Clears (every visible chunk repaints). `clear` was
        // resolved once above so the one-shot flag can't double-fire.
        let scene_load = if clear {
            wgpu::LoadOp::Clear(to_wgpu(base_color))
        } else {
            wgpu::LoadOp::Load
        };
        // Custom (material-DSL) pipeline pre-warm: the op walk below holds
        // both `&self.batch` and the persistent pass (which borrows
        // `&self.gpu`) while the cache needs `&mut self`. Collect the
        // distinct custom sources from the batch and resolve them here —
        // before the encoder pass opens — so the executor only hits the
        // immutable cache reader.
        {
            use std::collections::HashSet;
            let mut seen: HashSet<u64> = HashSet::new();
            let mut sources: Vec<String> = Vec::new();
            fn fnv(s: &str) -> u64 {
                let mut h: u64 = 0xcbf29ce484222325;
                for b in s.as_bytes() {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            for op in self.batch.ops.iter() {
                if let Op::BeginLayer { backdrop_stages, .. } = op {
                    for stage in backdrop_stages.iter() {
                        if let StageGpu::Custom(c) = stage {
                            let h = fnv(&c.wgsl);
                            if seen.insert(h) {
                                sources.push(c.wgsl.clone());
                            }
                        }
                    }
                }
            }
            for src in sources {
                self.custom_pipeline_for(&src);
            }
        }
        {
            // Bind groups referenced by an open pass must outlive it, so
            // the composite materials live in `kept` — declared before the
            // pass, dropped after it. The persistent pass opens first: it
            // records the frame's Clear/Load even when the first op turns
            // out to be a layer marker, and every marker arm re-establishes
            // a pass before returning, so content ops always find one open.
            let mut kept: Vec<wgpu::BindGroup> = Vec::new();
            let mut pass = Some(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("riode-wgpu-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.gpu.persistent_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: scene_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            }));
            let (sw, sh) = (self.gpu.config.width.max(1), self.gpu.config.height.max(1));
            // Open filter layers, outermost first (pool slot = depth).
            let mut stack: Vec<OpenLayer> = Vec::new();
            // Root offscreen layers opened this frame (device-px regions):
            // next frame's damage-base candidates (see `stale_root_layers`).
            let mut root_layer_regions: Vec<ResolvedRect> = Vec::new();
            for op in self.batch.ops.iter() {
                match op {
                    #[allow(unused_assignments)] // the `pass = None` drop is the point
                    Op::BeginLayer {
                        region,
                        params,
                        glass,
                        stages,
                        backdrop_stages,
                    } => {
                        // End the parent's pass first: wgpu borrows the
                        // encoder and its attachment view into the pass.
                        // An explicit assignment (not `take()`) — the drop
                        // of the old value ends those borrows here, before
                        // the next begin_render_pass.
                        pass = None;
                        let slot = stack.len();
                        while self.layer_pool.len() <= slot {
                            let i = self.layer_pool.len();
                            self.layer_pool.push(make_filter_target(
                                &self.gpu.device,
                                sw,
                                sh,
                                &format!("riode-wgpu-layer-{i}"),
                            ));
                        }
                        // Snapshot the parent's current pixels under the
                        // padded region — recorded after every parent
                        // draw so far (the pass above just ended) and
                        // before this layer's children (which draw into
                        // the layer offscreen, never the parent).
                        // Glass refracts it in-shader; a blend layer
                        // composites the finished layer against it (CSS
                        // backdrop = whatever painted beneath). Nested
                        // layers snapshot the enclosing layer's
                        // offscreen; root layers snapshot the persistent
                        // target (both COPY_SRC).
                        let blend = self.batch.filter_params[*params as usize].blend;
                        if glass.is_some() || blend != 0.0 || !backdrop_stages.is_empty() {
                            while self.glass_pool.len() <= slot {
                                let i = self.glass_pool.len();
                                self.glass_pool.push(make_glass_targets(
                                    &self.gpu.device,
                                    sw,
                                    sh,
                                    i,
                                ));
                            }
                            if let Some((cx, cy, cw, ch)) =
                                Self::scissor_rect(&Some(*region), sw, sh)
                            {
                                // Clear the pooled snapshot first: the
                                // copy fills only the padded region, but
                                // blur tails sample 3σ past it — stale
                                // texels from a previous glass layer at
                                // another position would shimmer in.
                                {
                                    let mut clear = enc.begin_render_pass(
                                        &wgpu::RenderPassDescriptor {
                                            label: Some("riode-wgpu-glass-snap-clear"),
                                            color_attachments: &[Some(
                                                wgpu::RenderPassColorAttachment {
                                                    view: &self.glass_pool[slot].snap.1,
                                                    resolve_target: None,
                                                    ops: wgpu::Operations {
                                                        load: wgpu::LoadOp::Clear(
                                                            wgpu::Color::TRANSPARENT,
                                                        ),
                                                        store: wgpu::StoreOp::Store,
                                                    },
                                                },
                                            )],
                                            depth_stencil_attachment: None,
                                            occlusion_query_set: None,
                                            timestamp_writes: None,
                                        },
                                    );
                                    clear.set_scissor_rect(0, 0, sw, sh);
                                }
                                let parent_tex = match stack.last() {
                                    Some(l) => &self.layer_pool[l.slot].0,
                                    None => &self.gpu.persistent,
                                };
                                enc.copy_texture_to_texture(
                                    wgpu::ImageCopyTexture {
                                        texture: parent_tex,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d {
                                            x: cx,
                                            y: cy,
                                            z: 0,
                                        },
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    wgpu::ImageCopyTexture {
                                        texture: &self.glass_pool[slot].snap.0,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d {
                                            x: cx,
                                            y: cy,
                                            z: 0,
                                        },
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    wgpu::Extent3d {
                                        width: cw,
                                        height: ch,
                                        depth_or_array_layers: 1,
                                    },
                                );
                            }
                        }
                        // Root offscreen layers composite into the
                        // persistent target, so their region must be
                        // repainted to base on the next Load frame
                        // (`stale_root_layers`). Nested layers composite
                        // into the enclosing offscreen, which clears on
                        // open every frame — no staleness there.
                        if stack.is_empty() {
                            root_layer_regions.push(*region);
                        }
                        stack.push(OpenLayer {
                            slot,
                            region: *region,
                            params: *params,
                            glass: *glass,
                            stages: stages.clone(),
                            backdrop_stages: backdrop_stages.clone(),
                        });
                        // Pool slots are reused across frames/layers, so
                        // every open starts from transparent.
                        pass = Some(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("riode-wgpu-layer"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &self.layer_pool[slot].1,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                        }));
                    }
                    #[allow(unused_assignments)] // the `pass = None` drop is the point
                    Op::EndLayer {
                        start,
                        count,
                        scissor,
                        shadow_start,
                        shadows,
                        spill_start,
                        spill_count,
                    } => {
                        // Close the layer's pass, run the filter passes,
                        // then reopen the parent (Load: every layer in the
                        // stack was cleared when it opened, and the
                        // persistent pass already recorded the frame's
                        // Clear/Load) and composite into it. The pass stays
                        // open afterwards so following content draws on.
                        // Explicit assignment — see BeginLayer above.
                        pass = None;
                        let Some(open) = stack.pop() else {
                            tracing::warn!(
                                "riode-wgpu: EndLayer without a matching BeginLayer"
                            );
                            // Restore the invariant content ops rely on.
                            pass = Some(enc.begin_render_pass(
                                &wgpu::RenderPassDescriptor {
                                    label: Some("riode-wgpu-scene"),
                                    color_attachments: &[Some(
                                        wgpu::RenderPassColorAttachment {
                                            view: &self.gpu.persistent_view,
                                            resolve_target: None,
                                            ops: wgpu::Operations {
                                                load: wgpu::LoadOp::Load,
                                                store: wgpu::StoreOp::Store,
                                            },
                                        },
                                    )],
                                    depth_stencil_attachment: None,
                                    occlusion_query_set: None,
                                    timestamp_writes: None,
                                },
                            ));
                            continue;
                        };
                        let fp = self
                            .batch
                            .filter_params
                            .get(open.params as usize)
                            .copied()
                            .unwrap_or(FilterGpu {
                                sigma: 0.0,
                                matrix: FilterGpu::IDENTITY,
                                blend: 0.0,
                            });
                        // Glass material for this layer (frame-local index,
                        // same contract as filter_params).
                        let gp = open
                            .glass
                            .and_then(|i| self.batch.glass_params.get(i as usize).copied());
                        // Ping targets for the filter chain, the glass
                        // backdrop blurs and any alpha shadow blurs.
                        // Allocated before the first borrow of either
                        // target so no arm reassigns mid-use.
                        if (!open.stages.is_empty()
                            || !open.backdrop_stages.is_empty()
                            || fp.sigma > 0.0
                            || !shadows.is_empty()
                            || gp.is_some())
                            && self.blur_ping.is_none()
                        {
                            self.blur_ping = Some((
                                make_filter_target(
                                    &self.gpu.device,
                                    sw,
                                    sh,
                                    "riode-wgpu-blur-ping0",
                                ),
                                make_filter_target(
                                    &self.gpu.device,
                                    sw,
                                    sh,
                                    "riode-wgpu-blur-ping1",
                                ),
                            ));
                        }
                        // Blend-mode shadow snapshot target: allocated
                        // before the first borrow of either target (same
                        // rule as ping) so no arm reassigns mid-use.
                        if shadows.iter().any(|d| d.blend != 0.0)
                            && self.shadow_snap.is_none()
                        {
                            self.shadow_snap = Some(make_shadow_snap(
                                &self.gpu.device,
                                sw,
                                sh,
                            ));
                        }
                        // Ordered stage executor: walk the lowered stack,
                        // ping-ponging through the layer offscreen and the
                        // blur scratch pair. `cur` tracks the current
                        // image; every scratch target is chosen distinct
                        // from its read source (no target is ever sampled
                        // and written in the same pass).
                        //
                        // Backdrop walk first: build the lens card from the
                        // parent snapshot. Blur/ColorMatrix before glass
                        // transform the snapshot; glass refracts the current
                        // backdrop into the card (`gt.out`); stages after
                        // glass transform the card. The snapshot itself is
                        // never written.
                        let mut bd = BdLoc::Snap;
                        let mut card: Option<CardLoc> = None;
                        let mut glass_ran = false;
                        let n_bd = open.backdrop_stages.len();
                        for (bi, stage) in open.backdrop_stages.iter().enumerate() {
                            let last = bi + 1 == n_bd;
                            match stage {
                                StageGpu::Blur { sigma } if card.is_none() => {
                                    let sigma = *sigma;
                                    if sigma <= 0.0 {
                                        continue;
                                    }
                                    let reach = 3.0 * sigma;
                                    let h_region = ResolvedRect::new(
                                        open.region.x - reach,
                                        open.region.y - reach,
                                        open.region.width + 2.0 * reach,
                                        open.region.height + 2.0 * reach,
                                    );
                                    let h_sc = Self::scissor_rect(&Some(h_region), sw, sh);
                                    let v_sc =
                                        Self::scissor_rect(&Some(open.region), sw, sh);
                                    if let (Some(h), Some(v)) = (h_sc, v_sc) {
                                        // H always lands on a ping (never
                                        // the snapshot, never the final
                                        // pre-blur input until the last
                                        // stage writes it directly).
                                        let a = Self::bd_dest(bd, false);
                                        let b = if last {
                                            BdLoc::GlassA
                                        } else {
                                            Self::bd_dest(a, false)
                                        };
                                        self.run_blur_pass(
                                            &mut enc,
                                            self.bd_view(open.slot, bd),
                                            self.bd_view(open.slot, a),
                                            sigma,
                                            (1.0, 0.0),
                                            h,
                                        );
                                        self.run_blur_pass(
                                            &mut enc,
                                            self.bd_view(open.slot, a),
                                            self.bd_view(open.slot, b),
                                            sigma,
                                            (0.0, 1.0),
                                            v,
                                        );
                                        bd = b;
                                    }
                                }
                                StageGpu::Blur { sigma } => {
                                    // Trailing blur: transform the lens card.
                                    let sigma = *sigma;
                                    if sigma <= 0.0 {
                                        continue;
                                    }
                                    let reach = 3.0 * sigma;
                                    let h_region = ResolvedRect::new(
                                        open.region.x - reach,
                                        open.region.y - reach,
                                        open.region.width + 2.0 * reach,
                                        open.region.height + 2.0 * reach,
                                    );
                                    let h_sc = Self::scissor_rect(&Some(h_region), sw, sh);
                                    let v_sc =
                                        Self::scissor_rect(&Some(open.region), sw, sh);
                                    if let (Some(h), Some(v)) = (h_sc, v_sc) {
                                        if let Some(c) = card {
                                            let a = Self::card_scratch(c);
                                            let b = if last {
                                                CardLoc::GtOut
                                            } else {
                                                Self::card_scratch(a)
                                            };
                                            self.run_blur_pass(
                                                &mut enc,
                                                self.card_view(open.slot, c),
                                                self.card_view(open.slot, a),
                                                sigma,
                                                (1.0, 0.0),
                                                h,
                                            );
                                            self.run_blur_pass(
                                                &mut enc,
                                                self.card_view(open.slot, a),
                                                self.card_view(open.slot, b),
                                                sigma,
                                                (0.0, 1.0),
                                                v,
                                            );
                                            card = Some(b);
                                        }
                                    }
                                }
                                StageGpu::ColorMatrix { matrix } if card.is_none() => {
                                    let v_sc =
                                        Self::scissor_rect(&Some(open.region), sw, sh);
                                    if let Some(v) = v_sc {
                                        let dest = Self::bd_dest(bd, last);
                                        let cm = ColorMatrixGpu::pack(matrix);
                                        self.run_composite_quad(
                                            &mut enc,
                                            self.bd_view(open.slot, bd),
                                            &cm,
                                            self.bd_view(open.slot, dest),
                                            *start..*start + *count,
                                            v,
                                            true,
                                            &mut kept,
                                        );
                                        bd = dest;
                                    }
                                }
                                StageGpu::ColorMatrix { matrix } => {
                                    // Trailing matrix: transform the lens card.
                                    let v_sc =
                                        Self::scissor_rect(&Some(open.region), sw, sh);
                                    if let Some(v) = v_sc {
                                        if let Some(c) = card {
                                            // Never src == dst in one pass:
                                            // a last-stage matrix on a card
                                            // already in `gt.out` goes through
                                            // a ping; finalize copies it back.
                                            let dest = if last && c != CardLoc::GtOut {
                                                CardLoc::GtOut
                                            } else {
                                                Self::card_scratch(c)
                                            };
                                            let cm = ColorMatrixGpu::pack(matrix);
                                            self.run_composite_quad(
                                                &mut enc,
                                                self.card_view(open.slot, c),
                                                &cm,
                                                self.card_view(open.slot, dest),
                                                *start..*start + *count,
                                                v,
                                                true,
                                                &mut kept,
                                            );
                                            card = Some(dest);
                                        }
                                    }
                                }
                                StageGpu::Glass => {
                                    if let Some(gp) = gp {
                                        let gt = &self.glass_pool[open.slot];
                                        let (sx, sy) = (gp.core[0], gp.core[1]);
                                        if let Some(rc) =
                                            Self::scissor_rect(&Some(open.region), sw, sh)
                                        {
                                            // Backdrop input: the walk's
                                            // current image; `sharp` (binding
                                            // 3) stays the raw snapshot.
                                            let mut backdrop: &wgpu::TextureView =
                                                self.bd_view(open.slot, bd);
                                            if sx.max(sy) > 0.05 {
                                                // H writes region ⊕ 3·max(σx, σy)
                                                // so V's edge taps (reach 3σy)
                                                // land on written texels whichever
                                                // axis is wider; V writes the
                                                // glass pre-blur input.
                                                let reach = 3.0 * sx.max(sy);
                                                let h_region = ResolvedRect::new(
                                                    open.region.x - reach,
                                                    open.region.y - reach,
                                                    open.region.width + 2.0 * reach,
                                                    open.region.height + 2.0 * reach,
                                                );
                                                if let Some(h) = Self::scissor_rect(
                                                    &Some(h_region),
                                                    sw,
                                                    sh,
                                                ) {
                                                    // H scratch must not alias
                                                    // the backdrop source.
                                                    let hscratch =
                                                        Self::bd_dest(bd, false);
                                                    self.run_blur_pass(
                                                        &mut enc,
                                                        backdrop,
                                                        self.bd_view(open.slot, hscratch),
                                                        sx,
                                                        (1.0, 0.0),
                                                        h,
                                                    );
                                                    self.run_blur_pass(
                                                        &mut enc,
                                                        self.bd_view(open.slot, hscratch),
                                                        &gt.a.1,
                                                        sy,
                                                        (0.0, 1.0),
                                                        rc,
                                                    );
                                                    backdrop = &gt.a.1;
                                                }
                                            }
                                            self.run_glass_pass(
                                                &mut enc,
                                                backdrop,
                                                &gt.snap.1,
                                                gp,
                                                &gt.out.1,
                                                rc,
                                            );
                                            glass_ran = true;
                                            card = Some(CardLoc::GtOut);
                                        }
                                    }
                                }
                                StageGpu::Custom(c) => {
                                    // Post-glass material surface: transform
                                    // the lens card in place (ping when the
                                    // card already sits in `gt.out` on a
                                    // last stage — never src == dst in one
                                    // pass; the finalize below copies a
                                    // pinged card back). Skipped when no
                                    // card exists yet (glass produced none)
                                    // — same guard as the trailing blur.
                                    // The pipeline was pre-resolved above
                                    // (pre-warm over the batch); `None`
                                    // = failed naga validation / build,
                                    // already warned once — drop the stage.
                                    if let (Some(cur), Some(pipe)) = (
                                        card,
                                        self.custom_pipeline_cached(&c.wgsl),
                                    )
                                    {
                                        if let Some(v) = Self::scissor_rect(
                                            &Some(open.region),
                                            sw,
                                            sh,
                                        ) {
                                            let dest = if last
                                                && cur != CardLoc::GtOut
                                            {
                                                CardLoc::GtOut
                                            } else {
                                                Self::card_scratch(cur)
                                            };
                                            self.run_custom_pass(
                                                &mut enc,
                                                &pipe,
                                                self.card_view(open.slot, cur),
                                                self.bd_view(open.slot, bd),
                                                &self.glass_pool[open.slot].snap.1,
                                                &c.uniforms,
                                                self.card_view(open.slot, dest),
                                                v,
                                            );
                                            card = Some(dest);
                                        }
                                    }
                                }
                                // Blend rides the final composite
                                // (`FilterGpu.blend` / the glass frame
                                // header); Shadow/ClipRound/GlassGroup
                                // never reach the executor.
                                _ => {}
                            }
                        }
                        // Finalize: the effected backdrop (the non-glass
                        // blend consumer) into the glass pre-blur input,
                        // and the lens card into its read-only home for
                        // the spill/merge consumers below. A skipped last
                        // stage (scissor miss) leaves the image in a ping.
                        let bd_ran = bd != BdLoc::Snap;
                        if bd_ran && bd != BdLoc::GlassA && !glass_ran {
                            if let Some(v) = Self::scissor_rect(&Some(open.region), sw, sh) {
                                self.run_composite_quad(
                                    &mut enc,
                                    self.bd_view(open.slot, bd),
                                    &ColorMatrixGpu::pack(&FilterGpu::IDENTITY),
                                    self.bd_view(open.slot, BdLoc::GlassA),
                                    *start..*start + *count,
                                    v,
                                    true,
                                    &mut kept,
                                );
                            }
                        }
                        if let Some(c) = card {
                            if c != CardLoc::GtOut {
                                if let Some(v) =
                                    Self::scissor_rect(&Some(open.region), sw, sh)
                                {
                                    self.run_composite_quad(
                                        &mut enc,
                                        self.card_view(open.slot, c),
                                        &ColorMatrixGpu::pack(&FilterGpu::IDENTITY),
                                        self.card_view(open.slot, CardLoc::GtOut),
                                        *start..*start + *count,
                                        v,
                                        true,
                                        &mut kept,
                                    );
                                }
                            }
                        }
                        let mut cur = CurLoc::Layer;
                        let pending_blend: Option<f32> =
                            if fp.blend != 0.0 { Some(fp.blend) } else { None };
                        // LightSpill: blurred copy of the finished card,
                        // composited only in the strip below the bottom
                        // edge with a downward shift, so the card's own
                        // colour spills softly onto the background with
                        // decreasing opacity (blur falloff), dimmer than
                        // the card. Drawn first at composite.
                        let mut glass_halo: Option<(&wgpu::TextureView, f32)> = None;
                        // Downward sample shift for the spill composite
                        // (device px): the blurred card's hottest line sits
                        // exactly on the silhouette edge — sampling ~1.5σ
                        // below it drops the white attachment band and opens
                        // the spill from already-fallen-off light.
                        let mut spill_tex_off = 0.0;
                        let v_sc = Self::scissor_rect(&Some(open.region), sw, sh);
                        for stage in open.stages.iter() {
                            match stage {
                                StageGpu::Blur { sigma } => {
                                    let sigma = *sigma;
                                    if sigma <= 0.0 {
                                        continue;
                                    }
                                    // H writes region⊕3σ so V's edge taps
                                    // land on written texels; V writes the
                                    // composite's region. A region that
                                    // misses the surface skips both.
                                    let reach = 3.0 * sigma;
                                    let h_region = ResolvedRect::new(
                                        open.region.x - reach,
                                        open.region.y - reach,
                                        open.region.width + 2.0 * reach,
                                        open.region.height + 2.0 * reach,
                                    );
                                    let h_sc = Self::scissor_rect(&Some(h_region), sw, sh);
                                    if let (Some(h), Some(v)) = (h_sc, v_sc) {
                                        let (a, b) = Self::stage_pair(cur);
                                        self.run_blur_pass(
                                            &mut enc,
                                            self.stage_view(open.slot, cur),
                                            self.stage_view(open.slot, a),
                                            sigma,
                                            (1.0, 0.0),
                                            h,
                                        );
                                        self.run_blur_pass(
                                            &mut enc,
                                            self.stage_view(open.slot, a),
                                            self.stage_view(open.slot, b),
                                            sigma,
                                            (0.0, 1.0),
                                            v,
                                        );
                                        cur = b;
                                    }
                                }
                                StageGpu::ColorMatrix { matrix } => {
                                    if let Some(v) = v_sc {
                                        let dest = Self::stage_scratch(cur);
                                        let cm = ColorMatrixGpu::pack(matrix);
                                        self.run_composite_quad(
                                            &mut enc,
                                            self.stage_view(open.slot, cur),
                                            &cm,
                                            self.stage_view(open.slot, dest),
                                            *start..*start + *count,
                                            v,
                                            true,
                                            &mut kept,
                                        );
                                        cur = dest;
                                    }
                                }
                                // Blend rides the final composite
                                // (`FilterGpu.blend`); glass lives on the
                                // backdrop side now. A stray Glass/Blend
                                // stage here (Rust-built stack) is inert.
                                _ => {}
                            }
                        }
                        // Lens card under the sheet: the backdrop walk
                        // produced the card; merge it UNDER the finished
                        // sheet (src-over associativity keeps this
                        // identical to sequential parent draws), with the
                        // LightSpill halo replayed from the final card
                        // first.
                        if glass_ran {
                            let gt = &self.glass_pool[open.slot];
                            let go: &wgpu::TextureView = &gt.out.1;
                            // LightSpill: blurred copy of the finished card,
                            // composited only in the strip below the bottom
                            // edge with a downward shift, so the card's own
                            // colour spills softly onto the background with
                            // decreasing opacity (blur falloff), dimmer than
                            // the card. Drawn first at composite.
                            if let Some(gp) = gp {
                                if let Some((strength, sigma, iters, length)) =
                                    light_spill_replay(&gp.data)
                                {
                                    if *spill_count > 0 {
                                        {
                                            use std::sync::atomic::{AtomicBool, Ordering};
                                            static SPILL_LOG: AtomicBool =
                                                AtomicBool::new(false);
                                            if !SPILL_LOG.swap(true, Ordering::Relaxed) {
                                                eprintln!(
                                                    "[spill v3] gain={strength} len={length} sigma={sigma} iters={iters} verts={spill_count}"
                                                );
                                            }
                                        }
                                        let reach = 3.0 * sigma;
                                        let h_region = ResolvedRect::new(
                                            open.region.x - reach,
                                            open.region.y - reach,
                                            open.region.width + 2.0 * reach,
                                            open.region.height + length + 2.0 * reach,
                                        );
                                        if let Some(h) =
                                            Self::scissor_rect(&Some(h_region), sw, sh)
                                        {
                                            // Scratch pair distinct from `cur`
                                            // (ping-pong across iterations;
                                            // `gt.out` is only ever read after
                                            // finalize, so the sharp card
                                            // survives).
                                            let (s0, s1) = Self::stage_pair(cur);
                                            let mut src: &wgpu::TextureView = go;
                                            for _ in 0..iters {
                                                self.run_blur_pass(
                                                    &mut enc,
                                                    src,
                                                    self.stage_view(open.slot, s0),
                                                    sigma,
                                                    (1.0, 0.0),
                                                    h,
                                                );
                                                self.run_blur_pass(
                                                    &mut enc,
                                                    self.stage_view(open.slot, s0),
                                                    self.stage_view(open.slot, s1),
                                                    sigma,
                                                    (0.0, 1.0),
                                                    h,
                                                );
                                                src = self.stage_view(open.slot, s1);
                                            }
                                            glass_halo = Some((
                                                self.stage_view(open.slot, s1),
                                                strength,
                                            ));
                                            spill_tex_off = sigma * 1.5;
                                        }
                                    } // end spill_count guard
                                }
                            }
                            // Merge under the current image: glass output
                            // into a cleared scratch, then the current image
                            // over it.
                            if let Some(v) = v_sc {
                                let quad = *start..*start + *count;
                                let dest = Self::stage_scratch(cur);
                                let ident = ColorMatrixGpu::pack(&FilterGpu::IDENTITY);
                                self.run_composite_quad(
                                    &mut enc,
                                    go,
                                    &ident,
                                    self.stage_view(open.slot, dest),
                                    quad.clone(),
                                    v,
                                    true,
                                    &mut kept,
                                );
                                self.run_composite_quad(
                                    &mut enc,
                                    self.stage_view(open.slot, cur),
                                    &ident,
                                    self.stage_view(open.slot, dest),
                                    quad,
                                    v,
                                    false,
                                    &mut kept,
                                );
                                cur = dest;
                            }
                        }

                        let parent_view = match stack.last() {
                            Some(l) => &self.layer_pool[l.slot].1,
                            None => &self.gpu.persistent_view,
                        };
                        // LightSpill composite, first onto the parent (own
                        // pass, Load): the blurred card through the
                        // feathered spill verts (vertex alpha shapes the
                        // fade; the matrix gain + downward sample shift
                        // keep it dim and free of the edge's hot line).
                        // Everything after this — alpha-silhouette shadows,
                        // lens, content — paints over it, so the spill sits
                        // behind the card AND behind any shadow it casts.
                        if let Some((halo, strength)) = glass_halo {
                            let mut cm = ColorMatrixGpu::pack(&[
                                1.0, 0.0, 0.0, 0.0, 0.0, //
                                0.0, 1.0, 0.0, 0.0, 0.0, //
                                0.0, 0.0, 1.0, 0.0, 0.0, //
                                0.0, 0.0, 0.0, strength, 0.0,
                            ]);
                            cm.tex_off = [0.0, spill_tex_off, 0.0, 0.0];
                            let sbuf = self.gpu.device.create_buffer_init(
                                &wgpu::util::BufferInitDescriptor {
                                    label: Some("riode-wgpu-spill-matrix"),
                                    contents: bytemuck::bytes_of(&cm),
                                    usage: wgpu::BufferUsages::UNIFORM,
                                },
                            );
                            kept.push(self.gpu.device.create_bind_group(
                                &wgpu::BindGroupDescriptor {
                                    label: Some("riode-wgpu-spill-material"),
                                    layout: &self.pipes.filter_layout,
                                    entries: &[
                                        wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: wgpu::BindingResource::TextureView(
                                                halo,
                                            ),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 1,
                                            resource: sbuf.as_entire_binding(),
                                        },
                                    ],
                                },
                            ));
                            pass = Some(enc.begin_render_pass(
                                &wgpu::RenderPassDescriptor {
                                    label: Some("riode-wgpu-spill"),
                                    color_attachments: &[Some(
                                        wgpu::RenderPassColorAttachment {
                                            view: parent_view,
                                            resolve_target: None,
                                            ops: wgpu::Operations {
                                                load: wgpu::LoadOp::Load,
                                                store: wgpu::StoreOp::Store,
                                            },
                                        },
                                    )],
                                    depth_stencil_attachment: None,
                                    occlusion_query_set: None,
                                    timestamp_writes: None,
                                },
                            ));
                            {
                                let p = Self::open_pass(&mut pass);
                                if Self::apply_scissor(p, scissor, sw, sh) {
                                    p.set_pipeline(&self.pipes.composite_pipeline);
                                    p.set_bind_group(0, &self.ortho_bg, &[]);
                                    p.set_vertex_buffer(0, self.vert_buf.slice(..));
                                    p.set_bind_group(1, kept.last().unwrap(), &[]);
                                    if *spill_count > 0 {
                                        p.draw(
                                            *spill_start..*spill_start + *spill_count,
                                            0..1,
                                        );
                                    }
                                }
                            }
                            pass = None;
                        }
                        // Alpha-silhouette shadow draws, painted onto the
                        // parent *behind* the content composite that
                        // follows: per shadow layer, blur the composite
                        // source at the layer sigma, then composite the
                        // tinted result at the layer offset (texel lookup
                        // shifts back by `-offset`). Each layer draws
                        // before the next blurs — the ping targets and
                        // the layer texture are reused across layers, so a
                        // deferred draw would sample a later layer's blur.
                        // No alpha-shadow quads for a glass layer: the
                        // shader draws its own Gaussian shadow into the
                        // optics output (uVar shadowRadius/shadowOpacity),
                        // which the glass composite below already carries.
                        // Non-glass layers keep casting from their
                        // finished silhouette (post-filter, in-layer
                        // clips baked in).
                        if !shadows.is_empty() && open.glass.is_none() {
                            // Blend-mode shadows composite against the
                            // parent snapshot (CSS `mix-blend-mode` needs
                            // the backdrop as input — same isolation the
                            // layer blend path gets at BeginLayer). Copied
                            // now: after the spill, before this layer's
                            // own shadow/content draws; sibling layers
                            // finished earlier are already in the parent.
                            // No pass is open here (each shadow draw opens
                            // its own below).
                            let shadow_snap: Option<&wgpu::TextureView> =
                                if shadows.iter().any(|d| d.blend != 0.0) {
                                    let snap = self
                                        .shadow_snap
                                        .as_ref()
                                        .expect("shadow snapshot ensured above");
                                    let parent_tex: &wgpu::Texture = match stack.last() {
                                        Some(l) => &self.layer_pool[l.slot].0,
                                        None => &self.gpu.persistent,
                                    };
                                    pass = None;
                                    enc.copy_texture_to_texture(
                                        wgpu::ImageCopyTexture {
                                            texture: parent_tex,
                                            mip_level: 0,
                                            origin: wgpu::Origin3d::ZERO,
                                            aspect: wgpu::TextureAspect::All,
                                        },
                                        wgpu::ImageCopyTexture {
                                            texture: &snap.0,
                                            mip_level: 0,
                                            origin: wgpu::Origin3d::ZERO,
                                            aspect: wgpu::TextureAspect::All,
                                        },
                                        wgpu::Extent3d {
                                            width: sw,
                                            height: sh,
                                            depth_or_array_layers: 1,
                                        },
                                    );
                                    Some(&snap.1)
                                } else {
                                    None
                                };
                            // Silhouette = the stage walk's current
                            // image (post Blur/ColorMatrix content, or
                            // the glass-merged card).
                            let silhouette: &wgpu::TextureView =
                                self.stage_view(open.slot, cur);
                            for (i, d) in shadows.iter().enumerate() {
                                let mut shadow_src: &wgpu::TextureView = silhouette;
                                if d.sigma > 0.05 {
                                    let reach = 3.0 * d.sigma;
                                    let h_region = ResolvedRect::new(
                                        open.region.x - reach,
                                        open.region.y - reach,
                                        open.region.width + 2.0 * reach,
                                        open.region.height + 2.0 * reach,
                                    );
                                    let h_sc = Self::scissor_rect(&Some(h_region), sw, sh);
                                    let v_sc =
                                        Self::scissor_rect(&Some(open.region), sw, sh);
                                    if let (Some(h), Some(v)) = (h_sc, v_sc) {
                                        // Scratch pair distinct from the
                                        // silhouette (same discipline as
                                        // the stage walk).
                                        let (ha, va) = Self::stage_pair(cur);
                                        self.run_blur_pass(
                                            &mut enc,
                                            silhouette,
                                            self.stage_view(open.slot, ha),
                                            d.sigma,
                                            (1.0, 0.0),
                                            h,
                                        );
                                        let v_dst: &wgpu::TextureView =
                                            self.stage_view(open.slot, va);
                                        self.run_blur_pass(
                                            &mut enc,
                                            self.stage_view(open.slot, ha),
                                            v_dst,
                                            d.sigma,
                                            (0.0, 1.0),
                                            v,
                                        );
                                        shadow_src = v_dst;
                                    }
                                }
                                let mut cm = ColorMatrixGpu::pack(&d.matrix);
                                cm.tex_off =
                                    [-d.offset[0], -d.offset[1], d.blend, 0.0];
                                let cbuf = self.gpu.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("riode-wgpu-shadow-matrix"),
                                        contents: bytemuck::bytes_of(&cm),
                                        usage: wgpu::BufferUsages::UNIFORM,
                                    },
                                );
                                // Blend-mode shadows composite through the
                                // blend pipeline against the snapshot taken
                                // above (`tex_off.z` is the mode id —
                                // `composite_blend.wgsl` reads it like the
                                // layer blend path); Normal keeps the plain
                                // composite untouched.
                                if d.blend != 0.0 {
                                    let snap_view = shadow_snap.expect(
                                        "blend shadow implies a snapshot",
                                    );
                                    kept.push(self.gpu.device.create_bind_group(
                                        &wgpu::BindGroupDescriptor {
                                            label: Some("riode-wgpu-shadow-blend-material"),
                                            layout: &self.pipes.blend_layout,
                                            entries: &[
                                                wgpu::BindGroupEntry {
                                                    binding: 0,
                                                    resource:
                                                        wgpu::BindingResource::TextureView(
                                                            shadow_src,
                                                        ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 1,
                                                    resource:
                                                        wgpu::BindingResource::TextureView(
                                                            snap_view,
                                                        ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 2,
                                                    resource: cbuf.as_entire_binding(),
                                                },
                                            ],
                                        },
                                    ));
                                } else {
                                    kept.push(self.gpu.device.create_bind_group(
                                        &wgpu::BindGroupDescriptor {
                                            label: Some("riode-wgpu-shadow-material"),
                                            layout: &self.pipes.filter_layout,
                                            entries: &[
                                                wgpu::BindGroupEntry {
                                                    binding: 0,
                                                    resource:
                                                        wgpu::BindingResource::TextureView(
                                                            shadow_src,
                                                        ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 1,
                                                    resource: cbuf.as_entire_binding(),
                                                },
                                            ],
                                        },
                                    ));
                                }
                                // Explicit assignment — see BeginLayer above.
                                pass = None;
                                pass = Some(enc.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("riode-wgpu-scene"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: parent_view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        occlusion_query_set: None,
                                        timestamp_writes: None,
                                    },
                                ));
                                {
                                    let p = Self::open_pass(&mut pass);
                                    if Self::apply_scissor(p, scissor, sw, sh) {
                                        if d.blend != 0.0 {
                                            // Blend pipeline (fixed-function
                                            // blending off): the shader
                                            // emits the full CSS composite
                                            // against the snapshot.
                                            p.set_pipeline(
                                                &self.pipes.blend_composite_pipeline,
                                            );
                                        } else {
                                            p.set_pipeline(&self.pipes.composite_pipeline);
                                        }
                                        p.set_bind_group(0, &self.ortho_bg, &[]);
                                        p.set_bind_group(1, kept.last().unwrap(), &[]);
                                        p.set_vertex_buffer(0, self.vert_buf.slice(..));
                                        let vs = *shadow_start + (i as u32) * 6;
                                        p.draw(vs..vs + 6, 0..1);
                                    }
                                }
                                pass = None;
                            }
                        }
                        // Final composite: the stage walk's current image
                        // onto the parent — one draw. The walk already
                        // applied every color stage and merged the glass
                        // card; what remains is an identity composite,
                        // or the blend pipeline when a trailing Blend
                        // stage asked for it and no glass ran (the optics
                        // card path blends in-shader). The material must
                        // outlive the open pass — into `kept`.
                        let blend_bg = match (pending_blend, glass_ran) {
                            (Some(m), false) => {
                                self.glass_pool.get(open.slot).map(|gt| {
                                    let mut cm =
                                        ColorMatrixGpu::pack(&FilterGpu::IDENTITY);
                                    cm.tex_off[2] = m;
                                    let bbuf = self.gpu.device.create_buffer_init(
                                        &wgpu::util::BufferInitDescriptor {
                                            label: Some("riode-wgpu-final-blend-matrix"),
                                            contents: bytemuck::bytes_of(&cm),
                                            usage: wgpu::BufferUsages::UNIFORM,
                                        },
                                    );
                                    kept.push(self.gpu.device.create_bind_group(
                                        &wgpu::BindGroupDescriptor {
                                            label: Some("riode-wgpu-final-blend-material"),
                                            layout: &self.pipes.blend_layout,
                                            entries: &[
                                                wgpu::BindGroupEntry {
                                                    binding: 0,
                                                    resource:
                                                        wgpu::BindingResource::TextureView(
                                                            self.stage_view(open.slot, cur),
                                                        ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 1,
                                                    resource:
                                                        wgpu::BindingResource::TextureView(
                                                            if bd_ran { &gt.a.1 } else { &gt.snap.1 },
                                                        ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 2,
                                                    resource: bbuf.as_entire_binding(),
                                                },
                                            ],
                                        },
                                    ));
                                    kept.len() - 1
                                })
                            }
                            _ => None,
                        };
                        pass = Some(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("riode-wgpu-scene"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: parent_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                        }));
                        let p = Self::open_pass(&mut pass);
                        if Self::apply_scissor(p, scissor, sw, sh) {
                            if let Some(bi) = blend_bg {
                                // Blend layer: the shader reads the
                                // snapshot and emits the full CSS
                                // composite — pipeline blend off.
                                p.set_pipeline(&self.pipes.blend_composite_pipeline);
                                p.set_bind_group(0, &self.ortho_bg, &[]);
                                p.set_vertex_buffer(0, self.vert_buf.slice(..));
                                p.set_bind_group(1, &kept[bi], &[]);
                                p.draw(*start..*start + *count, 0..1);
                            } else {
                                let ibuf = self.gpu.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("riode-wgpu-final-identity"),
                                        contents: bytemuck::bytes_of(&ColorMatrixGpu::pack(
                                            &FilterGpu::IDENTITY,
                                        )),
                                        usage: wgpu::BufferUsages::UNIFORM,
                                    },
                                );
                                kept.push(self.gpu.device.create_bind_group(
                                    &wgpu::BindGroupDescriptor {
                                        label: Some("riode-wgpu-final-material"),
                                        layout: &self.pipes.filter_layout,
                                        entries: &[
                                            wgpu::BindGroupEntry {
                                                binding: 0,
                                                resource: wgpu::BindingResource::TextureView(
                                                    self.stage_view(open.slot, cur),
                                                ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 1,
                                                resource: ibuf.as_entire_binding(),
                                            },
                                        ],
                                    },
                                ));
                                p.set_pipeline(&self.pipes.composite_pipeline);
                                p.set_bind_group(0, &self.ortho_bg, &[]);
                                p.set_vertex_buffer(0, self.vert_buf.slice(..));
                                p.set_bind_group(1, kept.last().unwrap(), &[]);
                                p.draw(*start..*start + *count, 0..1);
                            }
                        }
                    }
                    Op::BaseQuads { start, count } => {
                        let pass = Self::open_pass(&mut pass);
                        pass.set_scissor_rect(0, 0, sw, sh);
                        pass.set_pipeline(&self.pipes.rect_replace_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.vert_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Rects {
                        start,
                        count,
                        scissor,
                    } => {
                        let pass = Self::open_pass(&mut pass);
                        if !Self::apply_scissor(pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.rect_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.vert_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::RRects {
                        start,
                        count,
                        scissor,
                    } => {
                        let pass = Self::open_pass(&mut pass);
                        if !Self::apply_scissor(pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.rrect_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.rrect_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Shadows {
                        start,
                        count,
                        scissor,
                    } => {
                        let pass = Self::open_pass(&mut pass);
                        if !Self::apply_scissor(pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.shadow_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.shadow_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Glyphs {
                        start,
                        count,
                        scissor,
                        linear,
                    } => {
                        let pass = Self::open_pass(&mut pass);
                        if !Self::apply_scissor(pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.glyph_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_bind_group(
                            1,
                            if *linear {
                                &self.glyph_bg_linear
                            } else {
                                &self.glyph_bg_nearest
                            },
                            &[],
                        );
                        pass.set_vertex_buffer(0, self.glyph_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Images {
                        start,
                        count,
                        scissor,
                        bind,
                    } => {
                        let pass = Self::open_pass(&mut pass);
                        if !Self::apply_scissor(pass, scissor, sw, sh) {
                            continue;
                        }
                        let Some(img) = self.batch.image_binds.get(*bind as usize) else {
                            continue;
                        };
                        pass.set_pipeline(&self.pipes.image_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_bind_group(1, &img.bind, &[]);
                        pass.set_vertex_buffer(0, self.image_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                }
            }
            // Hand this frame's root layer regions to the next frame's
            // damage-base (field-disjoint from the borrows above).
            self.stale_root_layers = root_layer_regions;
            if !stack.is_empty() {
                tracing::warn!(
                    "riode-wgpu: {} filter layer(s) never closed (unbalanced BeginLayer/EndLayer)",
                    stack.len()
                );
            }
        }

        self.gpu.queue.submit(Some(enc.finish()));
        Ok(())
    }

    /// Persistent target size in pixels (quadrant size in quad-combine use).
    pub fn target_size(&self) -> (u32, u32) {
        (self.gpu.config.width.max(1), self.gpu.config.height.max(1))
    }

    /// Copies the persistent texture to CPU as tightly-packed RGBA bytes.
    ///
    /// Must follow [`Self::render_scene_offscreen`] (which submits the scene
    /// pass; this only adds the copy + a blocking map). Staging buffer is
    /// cached across frames and rebuilt on size change. The quad-combine
    /// backend composites these bytes with the other engines into one window.
    pub fn readback_rgba8(&mut self) -> Result<Vec<u8>, BackendError> {
        let (w, h) = self.target_size();
        // COPY_BYTES_PER_ROW_ALIGNMENT: rows are padded to 256B.
        let pitch = ((w * 4 + 255) / 256 * 256) as usize;
        let size = (pitch * h as usize) as u64;
        let buf = match &self.readback {
            Some((bw, bh, b)) if *bw == w && *bh == h => b,
            _ => {
                let b = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("riode-wgpu-readback"),
                    size: size.max(256),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                self.readback = Some((w, h, b));
                &self.readback.as_ref().unwrap().2
            }
        };
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("riode-wgpu-readback"),
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

    pub fn render_frame(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let surface = self.gpu.surface.as_ref().expect(
            "render_frame needs a window surface; headless contexts use render_scene_offscreen",
        );
        let frame = match surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(&self.gpu.device, &self.gpu.config);
                surface.get_current_texture().map_err(|e| {
                    BackendError::surface(format!("swapchain recovery failed: {e:?}"))
                })?
            }
            Err(e) => {
                return Err(BackendError::surface(format!(
                    "swapchain acquire failed: {e:?}"
                )))
            }
        };
        self.render_scene_offscreen(scene, base_color, damage)?;

        // Blit persistent target -> swapchain.
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("riode-wgpu-blit"),
            });
        {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bg = self
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("riode-wgpu-blit-bg"),
                    layout: &self.pipes.blit_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.gpu.persistent_view),
                    }],
                });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("riode-wgpu-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Transparent clear + premultiplied blend: a fresh
                        // swapchain texture starts unpainted, so a transparent
                        // window composites the desktop through. Opaque
                        // windows are unchanged (every pixel srcA = 1).
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipes.blit_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
        self.gpu.queue.submit(Some(enc.finish()));
        frame.present();
        Ok(())
    }
}
