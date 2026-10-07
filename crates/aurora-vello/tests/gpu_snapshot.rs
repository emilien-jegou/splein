// Headless GPU-vs-CPU snapshot comparison (needs a real GPU; #[ignore]d by default).
//
// Run with: cargo test -p aurora-vello -- --ignored
// Requires Vulkan loader visible (devenv provides it).
//
// Renders scenes through both rasterizers offscreen and compares color
// histograms. Any systematic absence (missing rows, black fills) shows up as
// gross histogram skew.

#[allow(dead_code)]
#[path = "../../aurora/examples/motion_reorder.rs"]
mod reorder;

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;
use aurora_tiny_skia::testing::Harness;
use aurora_tiny_skia::{HeadlessFrame, HeadlessRaster};
use vello::kurbo::Rect as KurboRect;
use vello::{AaConfig, AaSupport, RenderParams, Renderer as VelloEngine, RendererOptions};

fn rows(moved_dy: f32) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(760.0)
        .height(660.0)
        .fill(Color::hex(0xF4F5F7))
        .children([group()
            .direction(Direction::Vertical)
            .width(700.0)
            .height(600.0)
            .radius(12.0)
            .clip(true)
            .fill(Color::hex(0x1E1E24))
            .children([
                row("one", 0.0),
                row("two", 0.0),
                row("three", moved_dy),
                row("four", 0.0),
                row("five", 0.0),
            ])
            .into_element()])
}

fn row(label: &str, dy: f32) -> Element {
    group()
        .key(format!("row-{label}"))
        .width(680.0)
        .height(100.0)
        .radius(12.0)
        .fill(Color::hex(0x1019EC))
        .transform(Transform::from_translation(0.0, dy))
        .children([text(label.to_string())
            .size(14.0)
            .color(Color::WHITE)
            .into_element()])
        .into_element()
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    engine: VelloEngine,
}

impl Gpu {
    fn init() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
        }))
        .expect("no GPU adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .expect("no GPU device");
        let engine = VelloEngine::new(
            &device,
            RendererOptions {
                surface_format: None,
                use_cpu: false,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        )
        .expect("vello init");
        Self {
            device,
            queue,
            engine,
        }
    }

    /// Renders an aurora scene offscreen, returning raw RGBA8 pixels.
    fn render(&mut self, engine: &Engine, w: u32, h: u32) -> Vec<u8> {
        use aurora::scene::chunk::InheritedClips;
        use aurora_vello::command::{compile_vello_chunk, VelloImageCache};
        use aurora_vello::renderer::{chunk_placement_affine, local_viewport_bound};
        use aurora_vello::shadow::VelloShadowCache;
        use aurora_vello::svg::VelloSvgCache;
        use aurora_vello::text::VelloGlyphCache;
        use vello::kurbo::Affine;
        use vello::peniko::BlendMode;
        use vello::Scene as VelloScene;

        let mut scene = VelloScene::new();
        let mut tx_stack = vec![Affine::IDENTITY];
        let mut shadow_cache = VelloShadowCache::new();
        let mut svg_cache = VelloSvgCache::new();
        let mut image_cache = VelloImageCache::new();
        let mut glyph_cache = VelloGlyphCache::new();
        let bound = KurboRect::new(0.0, 0.0, w as f64, h as f64);
        for chunk in engine.scene().chunks.iter() {
            // Chunk-local streams: compile content in isolation, then place.
            let placement = chunk_placement_affine(&chunk.abs_origin, &chunk.placement);
            let local_bound = local_viewport_bound(&bound, &placement);
            let mut frag = VelloScene::new();
            compile_vello_chunk()
                .commands(&chunk.commands)
                .scene(&mut frag)
                .bound(&local_bound)
                .tx_stack(&mut tx_stack)
                .shadow_cache(&mut shadow_cache)
                .svg_cache(&mut svg_cache)
                .image_cache(&mut image_cache)
                .glyph_cache(&mut glyph_cache)
                .text_ctx(&engine.text_context())
                .call();
            for clip in chunk.inherited_clips.iter() {
                let shape = aurora_vello::path::build_kurbo_rounded_rect(&clip.rect, clip.radius);
                scene.push_layer(BlendMode::default(), 1.0, Affine::IDENTITY, &shape);
            }
            scene.append(&frag, Some(placement));
            for _ in chunk.inherited_clips.iter() {
                scene.pop_layer();
            }
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.engine
            .render_to_texture(
                &self.device,
                &self.queue,
                &scene,
                &view,
                &RenderParams {
                    base_color: vello::peniko::Color::rgba(0.957, 0.961, 0.969, 1.0),
                    width: w,
                    height: h,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("gpu render");

        let pad = w * 4;
        let stride = pad.div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (stride * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let s = (y * stride) as usize;
            out.extend_from_slice(&mapped[s..s + (w * 4) as usize]);
        }
        out
    }
}

fn hist(px: &[u8]) -> (usize, usize, usize) {    // (blue-ish accent, near-white text, near-black arena)
    let mut blue = 0;
    let mut white = 0;
    let mut dark = 0;
    for p in px.chunks_exact(4) {
        let (r, g, b) = (p[0] as i16, p[1] as i16, p[2] as i16);
        if (r - 0x10).abs() <= 24 && (g - 0x19).abs() <= 24 && (b - 0xEC).abs() <= 24 {
            blue += 1;
        }
        if r > 235 && g > 235 && b > 235 {
            white += 1;
        }
        if r < 60 && g < 60 && b < 60 {
            dark += 1;
        }
    }
    (blue, white, dark)
}

#[test]
#[ignore]
#[ntest::timeout(120_000)]
fn gpu_matches_cpu_on_translated_rows() {
    let mut gpu = Gpu::init();
    for (name, dy) in [("rest", 0.0f32), ("moved", 37.0f32)] {
        let mut engine = Engine::headless(760, 660);
        engine.mount(rows(dy));
        engine.frame();

        let mut raster = HeadlessRaster::new();
        let cpu = raster.draw_engine(&mut engine).data().to_vec();
        let gpu_px = gpu.render(&engine, 760, 660);

        let (cb, cw, cd) = hist(&cpu);
        let (gb, gw, gd) = hist(&gpu_px);
        eprintln!("{name}: cpu blue={cb} white={cw} dark={cd} | gpu blue={gb} white={gw} dark={gd}");
        assert!(
            gb > cb / 2,
            "{name}: gpu lost the blue rows (cpu {cb} vs gpu {gb})"
        );
    }
}

fn full_damage(w: u32, h: u32) -> aurora::foundation::DamageRegion {
    let mut region = aurora::foundation::DamageRegion::new();
    region.push(aurora::foundation::ResolvedRect::new(
        0.0,
        0.0,
        w as f32,
        h as f32,
    ));
    region
}

#[test]
#[ignore]
#[ntest::timeout(300_000)]
fn gpu_matches_cpu_with_live_hud() {
    use aurora::app::{AppExtension, OverlayCaps, VisualDebugger};
    use aurora::app::painter::OverlayPainter;
    use aurora::foundation::ResolvedRect;

    let mut gpu = Gpu::init();
    let mut h = Harness::page(reorder::Page, reorder::WIDTH, reorder::HEIGHT);
    assert!(h.settle(900), "page must settle");
    let mut worst = 1.0f32;
    // Sixty frames of evolving HUD telemetry (FPS/ms strings change constantly,
    // so glyph runs, shaping cache keys, and overlay damage are all live).
    for i in 0..60u32 {
        let engine = h.engine();
        let (w, hh) = engine.logical_size();
        let mut diag = engine.frame().diagnostics;
        // Simulate the telemetry the debugger derives from wall time.
        diag.timings.total = std::time::Duration::from_micros(8000 + (i * 137 % 9000) as u64);
        let text_ctx = engine.text_context();
        let caps = OverlayCaps {
            backend: "GPU",
            scanline_overlays: false,
            viewport: ResolvedRect::new(0.0, 0.0, w as f32, hh as f32),
        };
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = true;
        debugger.render_overlay(
            &mut OverlayPainter::new(engine.scene_mut()),
            &text_ctx,
            &caps,
            &diag,
        );

        let mut raster = HeadlessRaster::new();
        let full = full_damage(w, hh);
        let cpu = raster
            .draw(HeadlessFrame {
                scene: engine.scene(),
                damage: &full,
                size: (w, hh),
                text_ctx: &text_ctx,
            })
            .data()
            .to_vec();
        let gpu_px = gpu.render(engine, w, hh);
        let (cb, _, _) = hist(&cpu);
        let (gb, _, _) = hist(&gpu_px);
        if cb > 1000 {
            worst = worst.min(gb as f32 / cb as f32);
        }
        eprintln!("hud{i:02}: cpu blue={cb} gpu blue={gb}");
    }
    assert!(
        worst > 0.5,
        "gpu lost rows on some live-HUD frame (worst ratio {worst})"
    );
}
fn compare_frame(
    gpu: &mut Gpu,
    h: &mut Harness,
    tag: &str,
) -> (usize, usize, usize, usize, usize, usize) {
    let shot = h.snapshot();
    let cpu = shot.pixmap().data().to_vec();
    let engine = h.engine();
    let (w, hh) = engine.logical_size();
    let gpu_px = gpu.render(engine, w, hh);
    let (cb, cw, cd) = hist(&cpu);
    let (gb, gw, gd) = hist(&gpu_px);
    eprintln!("{tag}: cpu blue={cb} white={cw} dark={cd} | gpu blue={gb} white={gw} dark={gd}");
    (cb, cw, cd, gb, gw, gd)
}

fn inject_hud(h: &mut Harness, fps: u32) {
    use aurora::app::{AppExtension, OverlayCaps, VisualDebugger};
    use aurora::app::painter::OverlayPainter;
    use aurora::foundation::ResolvedRect;

    let engine = h.engine();
    let (w, hh) = engine.logical_size();
    let mut diag = engine.frame().diagnostics;
    diag.timings.total = std::time::Duration::from_millis((fps % 17) as u64);
    let text_ctx = engine.text_context();
    let caps = OverlayCaps {
        backend: "GPU",
        scanline_overlays: false,
        viewport: ResolvedRect::new(0.0, 0.0, w as f32, hh as f32),
    };
    let mut debugger = VisualDebugger::new();
    debugger.show_hud = true;
    debugger.render_overlay(
        &mut OverlayPainter::new(engine.scene_mut()),
        &text_ctx,
        &caps,
        &diag,
    );
}

#[test]
#[ignore]
#[ntest::timeout(300_000)]
fn gpu_matches_cpu_drag_with_hud() {
    let mut gpu = Gpu::init();
    let mut h = Harness::page(reorder::Page, reorder::WIDTH, reorder::HEIGHT);
    assert!(h.settle(900), "page must settle");

    // Full gesture under a live HUD: press Baseline, drag down across three rows
    // in small increments (each step can trigger a reorder + FLIP set) with fresh
    // overlay telemetry every frame, hold, release, then let every spring settle.
    h.press("row-Baseline");
    let start = h.located("row-Baseline").center();
    let mut worst_ratio = 1.0f32;
    for i in 1..=24 {
        h.pointer(
            aurora::foundation::Point::new(start.x, start.y + i as f32 * 15.0),
            aurora::input::PointerPhase::Drag,
        );
        h.step();
        inject_hud(&mut h, 50 + i);
        let (cb, _, _, gb, _, _) = compare_frame(&mut gpu, &mut h, &format!("drag{i:02}"));
        if cb > 1000 {
            worst_ratio = worst_ratio.min(gb as f32 / cb as f32);
        }
    }
    h.release_at();
    for i in 0..30 {
        h.step();
        inject_hud(&mut h, 60 + (i % 7));
        let (cb, _, _, gb, _, _) = compare_frame(&mut gpu, &mut h, &format!("settle{i:02}"));
        if cb > 1000 {
            worst_ratio = worst_ratio.min(gb as f32 / cb as f32);
        }
    }
    assert!(
        worst_ratio > 0.5,
        "gpu lost rows on some drag+HUD frame (worst blue ratio {worst_ratio})"
    );
}

#[test]
#[ignore]
#[ntest::timeout(120_000)]
fn gpu_matches_cpu_mid_drag() {
    let mut gpu = Gpu::init();
    let mut h = Harness::page(reorder::Page, reorder::WIDTH, reorder::HEIGHT);
    assert!(h.settle(900), "page must settle");
    // Grab Baseline and hold it two rows down: the held row follows the pointer
    // (compositor transform + elevation) while the others are mid-FLIP. This is
    // the exact frame shape that flickers/vanishes on --gpu.
    h.drag_holding("row-Baseline", 0.0, 120.0, 6);
    h.step();

    let shot = h.snapshot();
    let cpu = shot.pixmap().data().to_vec();
    let engine = h.engine();
    let (w, hh) = engine.logical_size();
    let gpu_px = gpu.render(engine, w, hh);

    let (cb, cw, cd) = hist(&cpu);
    let (gb, gw, gd) = hist(&gpu_px);
    eprintln!("drag: cpu blue={cb} white={cw} dark={cd} | gpu blue={gb} white={gw} dark={gd}");
    std::fs::write("/tmp/gpu_drag_cpu.bin", &cpu).unwrap();
    std::fs::write("/tmp/gpu_drag_gpu.bin", &gpu_px).unwrap();
    assert!(
        gb > cb / 2,
        "gpu lost the blue rows mid-drag (cpu {cb} vs gpu {gb})"
    );
}

#[test]
#[ignore]
#[ntest::timeout(120_000)]
fn gpu_matches_cpu_on_reorder_page() {
    let mut gpu = Gpu::init();
    let mut h = Harness::page(reorder::Page, reorder::WIDTH, reorder::HEIGHT);
    assert!(h.settle(900), "page must settle");
    // CPU pixels come from the harness's own retained raster: a fresh raster
    // replaying post-settle (empty) damage would just return transparency.
    let shot = h.snapshot();
    let cpu = shot.pixmap().data().to_vec();
    let engine = h.engine();
    let (w, hh) = engine.logical_size();
    let gpu_px = gpu.render(engine, w, hh);

    let (cb, cw, cd) = hist(&cpu);
    let (gb, gw, gd) = hist(&gpu_px);
    eprintln!("page: cpu blue={cb} white={cw} dark={cd} | gpu blue={gb} white={gw} dark={gd}");
    std::fs::write("/tmp/gpu_page_cpu.bin", &cpu).unwrap();
    std::fs::write("/tmp/gpu_page_gpu.bin", &gpu_px).unwrap();
    assert!(
        gb > cb / 2,
        "gpu lost the blue rows (cpu {cb} vs gpu {gb})"
    );
}
