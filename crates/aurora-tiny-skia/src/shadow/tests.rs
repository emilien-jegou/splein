// Single responsibility: Structural harness for the 9-patch shadow pipeline.
//
// These assert mathematical invariants — transpose symmetry, rim decay,
// plateau coverage, monotonic falloff, mirror-equal blits — never golden
// pixels. A green run means the geometry is right; it says nothing about
// taste, only about arithmetic.

use super::patch::{blit_9patch, generate_patch, ShadowPatch};
use super::ShadowRasterizer;
use aurora::foundation::{
    BlendMode, Color, FalloffCurve, LightEnvironment, Radius, ResolvedRect, Shadow,
};
use tiny_skia::{Pixmap, Transform};

/// Bare shadow with one layer of the given geometry; the renderer reads
/// the same fields through the same `layer_geometry`.
fn shadow_with_layer(softness: f32, extent: f32) -> Shadow {
    Shadow::layered(Color::BLACK).ease_out(softness, extent, 1.0)
}

/// Renders one full shadow through the real rasterizer loop (offsets,
/// pads, opacity, cache) onto an empty canvas.
fn render_shadow(shadow: &Shadow, rect: &ResolvedRect) -> Pixmap {
    let mut raster = ShadowRasterizer::new();
    let mut pix = Pixmap::new(500, 500).unwrap();
    raster.render(
        &mut pix.as_mut(),
        rect,
        Radius::Scalar(8.0),
        shadow,
        &LightEnvironment::DEFAULT,
        1.0,
        Transform::identity(),
        None,
    );
    pix
}

fn pix_alpha(pix: &Pixmap, x: i32, y: i32) -> u8 {
    let w = pix.width() as i32;
    pix.data()[((y * w + x) * 4 + 3) as usize]
}

/// Single big layer: umbra core carries the layer opacity, rim is empty.
#[test]
fn render_single_big_sane() {
    let shadow = Shadow::layered(Color::rgba(
        122.0 / 255.0,
        61.0 / 255.0,
        143.0 / 255.0,
        0.15,
    ))
    .drop(8.0, 16.0, 16.0);
    let rect = ResolvedRect::new(100.0, 100.0, 150.0, 150.0);
    let pix = render_shadow(&shadow, &rect);
    // Shadow bbox: origin (100+8-48, 100+16-48) = (60,68), footprint 246.
    // Deep umbra (card+offset interior): full layer opacity ~0.15*255.
    let core = pix_alpha(&pix, 183, 191);
    assert!(
        (25..=50).contains(&core),
        "umbra core must carry one layer opacity, got {core}"
    );
    // Past the rim: empty.
    assert!(pix_alpha(&pix, 50, 191) <= 12, "rim must be empty");
    assert!(pix_alpha(&pix, 320, 191) <= 12, "rim must be empty");
}

/// Five stacked layers compose to a darker core than any single one: the
/// loop must render every layer exactly once.
#[test]
fn render_layers_compose() {
    let single = Shadow::layered(Color::rgba(0.0, 0.0, 0.0, 0.15)).drop(8.0, 16.0, 16.0);
    let stacked = Shadow::layered(Color::rgba(0.0, 0.0, 0.0, 0.15))
        .drop(0.5, 1.0, 1.0)
        .drop(1.0, 2.0, 2.0)
        .drop(2.0, 4.0, 4.0)
        .drop(4.0, 8.0, 8.0)
        .drop(8.0, 16.0, 16.0);
    let rect = ResolvedRect::new(100.0, 100.0, 150.0, 150.0);
    // Sample point inside every layer's umbra: card interior near its
    // top-left stays covered under all five small offsets.
    let (sx, sy) = (140, 140);
    let a_single = pix_alpha(&render_shadow(&single, &rect), sx, sy);
    let a_stack = pix_alpha(&render_shadow(&stacked, &rect), sx, sy);
    assert!(
        a_stack as i16 - a_single as i16 >= 60,
        "stacked core ({a_stack}) must exceed single layer ({a_single})"
    );
}

/// Spread expands the footprint: 10px past the spread-free rim (which is
/// empty) the spread shadow still carries coverage, and its umbra core
/// stays solid.
#[test]
fn render_spread_expands_footprint() {
    let rect = ResolvedRect::new(100.0, 100.0, 150.0, 150.0);
    let plain = Shadow::web_like(0.0, 0.0, 12.0, 0.0, Color::BLACK);
    let grown = Shadow::web_like(0.0, 0.0, 12.0, 10.0, Color::BLACK);
    let mid_y = 175;
    // Plain kernel support (2x6) ends 12px past the card edge: at 14px out
    // it is exactly empty. The grown shape edge sits 10px further out, so
    // the same pixel is 4px inside strong falloff.
    // Plain kernel support (±12) ends before x=264: exactly empty.
    let px = render_shadow(&plain, &rect);
    assert_eq!(pix_alpha(&px, 250 + 14, mid_y), 0);
    // Grown edge sits 10px further out; the same pixel is mid-falloff.
    let gx = render_shadow(&grown, &rect);
    assert!(pix_alpha(&gx, 250 + 14, mid_y) >= 40);
    // Umbra core solid in both.
    assert_eq!(pix_alpha(&px, 175, mid_y), 255);
    assert_eq!(pix_alpha(&gx, 175, mid_y), 255);
}

/// Rendering twice (warm cache) is byte-identical to the first render.
#[test]
fn render_cache_is_deterministic() {
    let shadow = Shadow::web_like(3.0, 6.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35));
    let rect = ResolvedRect::new(100.0, 100.0, 150.0, 150.0);
    let mut raster = ShadowRasterizer::new();
    let (mut p1, mut p2) = (
        Pixmap::new(500, 500).unwrap(),
        Pixmap::new(500, 500).unwrap(),
    );
    for pix in [&mut p1, &mut p2] {
        raster.render(
            &mut pix.as_mut(),
            &rect,
            Radius::Scalar(8.0),
            &shadow,
            &LightEnvironment::DEFAULT,
            1.0,
            Transform::identity(),
            None,
        );
    }
    assert_eq!(p1.data(), p2.data(), "cache must not alter output");
}

fn make_patch(radius: f32, softness: f32, extent: f32) -> (ShadowPatch, usize, usize) {
    // Same derivation the renderer uses — never duplicated here.
    let shadow = shadow_with_layer(softness, extent);
    let layer = &shadow.layers()[0];
    let (pad, cs, dim) = super::patch::layer_geometry(radius, &shadow, layer);
    let lut = FalloffCurve::Linear.lut();
    let (mut a, mut b, mut t) = (
        vec![0u8; dim * dim],
        vec![0u8; dim * dim],
        vec![0u8; dim * dim],
    );
    let patch = generate_patch(
        &mut a, &mut b, &mut t, radius, softness, &lut, pad, cs, dim, 0.0,
    );
    (patch, cs, dim)
}

/// Builds a patch with blur forced to pass through (radius 0 copies
/// src→dst; the Linear LUT is identity), exposing the raw occluder shape.
fn make_patch_raw(radius: f32) -> (ShadowPatch, usize, usize) {
    let shadow = shadow_with_layer(1.0, 3.0);
    let layer = &shadow.layers()[0];
    let (pad, cs, dim) = super::patch::layer_geometry(radius, &shadow, layer);
    let lut = FalloffCurve::Linear.lut();
    let (mut a, mut b, mut t) = (
        vec![0u8; dim * dim],
        vec![0u8; dim * dim],
        vec![0u8; dim * dim],
    );
    // NB: blur 0.0 selects the copy-through path; nothing else changes.
    let patch = generate_patch(&mut a, &mut b, &mut t, radius, 0.0, &lut, pad, cs, dim, 0.0);
    (patch, cs, dim)
}

fn at(patch: &ShadowPatch, dim: usize, x: usize, y: usize) -> u8 {
    patch.mask[y * dim + x]
}

/// Plateau must hold at any blur radius: infinite runs never dilute, so the
/// notch interior keeps full coverage. (Regression: finite tips diluted to
/// ghosts whenever blur exceeded the tip size.)
#[test]
fn plateau_holds_for_large_blur() {
    for &(softness, extent) in &[(1.0f32, 1.0f32), (6.0, 6.0), (8.0, 8.0), (16.0, 16.0)] {
        let (patch, cs, dim) = make_patch(8.0, softness, extent);
        // Deep interior is the anti-dilution probe: every ±2r window
        // pixel is solid run there. A finite 2r tip scores ~15; infinite
        // runs score ~250. Clamped into range for small patches.
        let q = (cs + 24).min(dim - 2);
        assert!(
            at(&patch, dim, q, q) >= 220,
            "deep interior must hold plateau (soft={softness})"
        );
        // Deep run interior (away from every shape boundary) is plateau.
        assert!(
            at(&patch, dim, dim - 1, dim - 1) >= 250,
            "run interior must stay solid (soft={softness})"
        );
    }
}

/// The outside borders (top row, left column) must be empty: blur support
/// never reaches past the shape there (pad covers 3x softness), so rims
/// fade out. Run-side borders are solid by design (infinite runs).
#[test]
fn rim_decays_to_zero() {
    for &(softness, extent) in &[(1.0f32, 1.0f32), (6.0, 6.0), (16.0, 16.0)] {
        let (patch, _cs, dim) = make_patch(8.0, softness, extent);
        let mut worst = 0u8;
        for i in 0..dim {
            worst = worst.max(at(&patch, dim, i, 0));
            worst = worst.max(at(&patch, dim, 0, i));
        }
        assert!(
            worst <= 12,
            "outside border must be empty (soft={softness}, worst={worst})"
        );
    }
}

/// The runs-shape is symmetric across the diagonal; the separable fixed-point
/// blur preserves that up to rounding wobble (division rounding accumulates
/// differently per axis).
#[test]
fn mask_is_transpose_symmetric() {
    let (patch, _cs, dim) = make_patch(8.0, 16.0, 16.0);
    for y in 0..dim {
        for x in 0..dim {
            let (a, b) = (at(&patch, dim, x, y), at(&patch, dim, y, x));
            assert!(
                a.abs_diff(b) <= 2,
                "transpose symmetry broken at ({x},{y}): {a} vs {b}"
            );
        }
    }
}

/// Far corner empty, notch interior solid, diagonal falls monotonically
/// (fixed-point wobble bounded).
#[test]
fn diagonal_profile_is_sane() {
    let (patch, cs, dim) = make_patch(8.0, 6.0, 6.0);
    assert_eq!(at(&patch, dim, 0, 0), 0, "far corner must be empty");
    let mut prev = 0u8;
    let mut worst_drop = 0i16;
    for t in 0..cs {
        let v = at(&patch, dim, t, t);
        worst_drop = worst_drop.max(prev as i16 - v as i16);
        prev = prev.max(v);
    }
    assert!(
        worst_drop <= 4,
        "diagonal must not invert (worst drop {worst_drop})"
    );
    // Solid input tops out at 254 through the truncating fixed-point blur.
    // Probed past the notch (interior, away from transition zones).
    assert!(
        at(&patch, dim, cs + 6, cs + 6) >= 220,
        "notch interior solid"
    );
}

struct Blit {
    pix: Pixmap,
    ox: i32,
    oy: i32,
    total_w: i32,
    total_h: i32,
}

/// Blits one layer of `shadow` for `rect` onto an empty canvas, returning
/// the pixels plus the shadow-space origin and footprint.
fn blit_layer(shadow: &Shadow, rect: &ResolvedRect) -> Blit {
    let light = LightEnvironment::DEFAULT;
    let layer = &shadow.layers()[0];
    let (ox, oy) = shadow.layer_offset(&light, layer);
    let pad = shadow.layer_pad(layer);
    let (patch, _cs, _dim) = make_patch(8.0, layer.softness, layer.extent);
    let mut pix = Pixmap::new(400, 400).unwrap();
    blit_9patch(
        &mut pix.as_mut(),
        rect,
        ox,
        oy,
        pad,
        Color::BLACK,
        shadow.color.a * layer.opacity,
        BlendMode::Normal,
        Transform::identity(),
        &patch,
        None,
    );
    let total_w = (rect.width + 2.0 * pad.ceil()).round() as i32;
    let total_h = (rect.height + 2.0 * pad.ceil()).round() as i32;
    Blit {
        pix,
        ox: (rect.x + ox - pad.ceil()).round() as i32,
        oy: (rect.y + oy - pad.ceil()).round() as i32,
        total_w,
        total_h,
    }
}

fn alpha_of(pix: &Pixmap, x: i32, y: i32) -> u8 {
    let w = pix.width() as i32;
    pix.data()[((y * w + x) * 4 + 3) as usize]
}

/// Seam continuity: walking across any 9-patch seam must show no step.
/// The corner-slice/center-fill seam once juxtaposed a still-ramping
/// profile against flat plateau (~100-level steps forming a dark square);
/// the slice now extends past the inward support so both sides agree.
#[test]
fn seams_show_no_step() {
    let shadow = Shadow::web_like(0.0, 0.0, 16.0, 0.0, Color::BLACK);
    let rect = ResolvedRect::new(100.0, 100.0, 150.0, 150.0);
    let layer = &shadow.layers()[0];
    let (_pad, cs, _dim) = super::patch::layer_geometry(8.0, &shadow, layer);
    let cs = cs as i32;
    let blit = blit_layer(&shadow, &rect);
    let total_w = blit.total_w;
    let total_h = blit.total_h;
    let mut worst = 0u8;
    // Vertical seams (strip/center borders), full spans.
    for &sx in &[blit.ox + cs, blit.ox + total_w - cs] {
        for y in blit.oy..blit.oy + total_h {
            let l = alpha_of(&blit.pix, sx - 1, y);
            let r = alpha_of(&blit.pix, sx, y);
            worst = worst.max(l.abs_diff(r));
        }
    }
    // Horizontal seams.
    for &sy in &[blit.oy + cs, blit.oy + total_h - cs] {
        for x in blit.ox..blit.ox + total_w {
            let t = alpha_of(&blit.pix, x, sy - 1);
            let b = alpha_of(&blit.pix, x, sy);
            worst = worst.max(t.abs_diff(b));
        }
    }
    assert!(worst <= 24, "seam step must stay small, got {worst}");
}

/// Zero-offset shadow is mirror-equal left/right and top/bottom up to the
/// mask's fixed-point wobble: mirrored positions run the same integer math
/// on near-identical inputs, so outputs agree within a few levels. A hole
/// (0 vs ~90) dwarfs that tolerance.
#[test]
fn blit_is_mirror_symmetric() {
    let shadow = Shadow::web_like(0.0, 0.0, 12.0, 0.0, Color::BLACK);
    let rect = ResolvedRect::new(100.0, 100.0, 120.0, 72.0);
    let blit = blit_layer(&shadow, &rect);
    assert_eq!(blit.total_w % 2, 0);
    assert_eq!(blit.total_h % 2, 0);
    for y in 0..blit.total_h {
        for x in 0..blit.total_w / 2 {
            let l = alpha_of(&blit.pix, blit.ox + x, blit.oy + y);
            let r = alpha_of(&blit.pix, blit.ox + blit.total_w - 1 - x, blit.oy + y);
            assert!(
                l.abs_diff(r) <= 4,
                "left/right mismatch at ({x},{y}): {l} vs {r}"
            );
        }
    }
    for y in 0..blit.total_h / 2 {
        for x in 0..blit.total_w {
            let t = alpha_of(&blit.pix, blit.ox + x, blit.oy + y);
            let b = alpha_of(&blit.pix, blit.ox + x, blit.oy + blit.total_h - 1 - y);
            assert!(
                t.abs_diff(b) <= 4,
                "top/bottom mismatch at ({x},{y}): {t} vs {b}"
            );
        }
    }
}

/// An offset shadow is mirror-equal about its OWN center: translation
/// invariance of the blit. Any offset-dependent hole breaks this.
#[test]
fn offset_blit_is_symmetric_about_its_center() {
    let shadow = Shadow::web_like(8.0, 16.0, 12.0, 0.0, Color::BLACK);
    let rect = ResolvedRect::new(100.0, 100.0, 120.0, 72.0);
    let blit = blit_layer(&shadow, &rect);
    assert_eq!(blit.total_w % 2, 0);
    assert_eq!(blit.total_h % 2, 0);
    for y in 0..blit.total_h {
        for x in 0..blit.total_w / 2 {
            let l = alpha_of(&blit.pix, blit.ox + x, blit.oy + y);
            let r = alpha_of(&blit.pix, blit.ox + blit.total_w - 1 - x, blit.oy + y);
            assert!(
                l.abs_diff(r) <= 4,
                "offset left/right mismatch at ({x},{y}): {l} vs {r}"
            );
        }
    }
}

/// Middle row decays monotonically from the umbra outward with no hollow
/// band and no solid slab past the falloff end.
#[test]
fn middle_row_decays_monotonically() {
    let shadow = Shadow::web_like(0.0, 0.0, 12.0, 0.0, Color::BLACK);
    let rect = ResolvedRect::new(100.0, 100.0, 120.0, 72.0);
    let blit = blit_layer(&shadow, &rect);
    let mid_y = blit.oy + blit.total_h / 2;
    let cx = blit.ox + blit.total_w / 2;
    let mut prev = 255u8;
    let mut worst_rise = 0i16;
    // From well inside the umbra to past the rim.
    for x in cx..blit.ox + blit.total_w {
        let v = alpha_of(&blit.pix, x, mid_y);
        worst_rise = worst_rise.max(v as i16 - prev as i16);
        prev = prev.min(v);
    }
    assert!(
        worst_rise <= 4,
        "falloff must not rise outward (rise {worst_rise})"
    );
    // Rim pixel is empty: no slab past the falloff.
    let rim = alpha_of(&blit.pix, blit.ox + blit.total_w - 1, mid_y);
    assert!(rim <= 12, "rim must be empty, got {rim}");
    // Umbra core is solid.
    assert_eq!(alpha_of(&blit.pix, cx, mid_y), 255);
}

/// Raw shape probe with blur disabled: the wrapper copies through on
/// radius 0, exposing the occluder geometry as exact binary. Arc center
/// (11,11) r=8 in a dim-22 patch: inside points solid, outside empty.
#[test]
fn raw_shape_matches_spec() {
    let (patch, _cs, dim) = make_patch_raw(8.0);
    assert_eq!(dim, 26);
    // Inside the arc.
    assert_eq!(at(&patch, dim, 8, 4), 255);
    assert_eq!(at(&patch, dim, 11, 11), 255);
    // Run interiors.
    assert_eq!(at(&patch, dim, 15, 11), 255);
    assert_eq!(at(&patch, dim, 11, 15), 255);
    // Cutout: inside the notch square but outside the arc.
    assert_eq!(at(&patch, dim, 4, 4), 0);
    assert_eq!(at(&patch, dim, 5, 4), 0);
    // Outside the shape entirely.
    assert_eq!(at(&patch, dim, 0, 11), 0);
    assert_eq!(at(&patch, dim, 11, 0), 0);
}

/// Zero-radius cards cast square shadows: no arc is cut from the quadrant.
/// Same probe point reads solid in the square patch, cut out in arced one.
#[test]
fn zero_radius_casts_square() {
    // r=0: pad=3, k=1, cs=5, dim=10. (8,8) is deep solid interior.
    let (square, _cs, sdim) = make_patch(0.0, 1.0, 1.0);
    assert_eq!(sdim, 10);
    let sq = at(&square, sdim, 8, 8);
    assert!(sq >= 220, "square interior solid, got {sq}");
    // r=8: pad=3, k=1, cs=13, dim=26. Arc center (11,11); probe (8,8) is
    // well inside the cutout (dist ~4.2 from center, outside r=8... no:
    // dist((8,8),(11,11)) = 4.24 < 8 means INSIDE the arc). Probe the
    // cutout instead at (4,4): dist 9.9, outside the arc.
    let (round, _rcs, rdim) = make_patch(8.0, 1.0, 1.0);
    assert_eq!(rdim, 26);
    let cut = at(&round, rdim, 4, 4);
    let sq_same = at(&square, sdim, 4, 4);
    assert!(
        sq_same > cut + 64,
        "square ({sq_same}) must exceed rounded cutout ({cut})"
    );
    // Far corner empty in both.
    assert_eq!(at(&square, sdim, 0, 0), 0);
    assert_eq!(at(&round, rdim, 0, 0), 0);
}

/// Direct-path replay determinism: a second identical render must hit the
/// coverage cache and produce byte-identical pixels (small rect forces the
/// direct path, bypassing 9-patch).
#[test]
fn direct_cache_replay_is_identical() {
    let shadow = shadow_with_layer(6.0, 12.0);
    // Small enough to take the direct path regardless of patch_dim.
    let rect = ResolvedRect::new(200.0, 200.0, 20.0, 12.0);
    let mut raster = ShadowRasterizer::new();
    let mut first = Pixmap::new(500, 500).unwrap();
    raster.render(
        &mut first.as_mut(),
        &rect,
        Radius::Scalar(4.0),
        &shadow,
        &LightEnvironment::DEFAULT,
        1.0,
        Transform::identity(),
        None,
    );
    let mut second = Pixmap::new(500, 500).unwrap();
    raster.render(
        &mut second.as_mut(),
        &rect,
        Radius::Scalar(4.0),
        &shadow,
        &LightEnvironment::DEFAULT,
        1.0,
        Transform::identity(),
        None,
    );
    // The first render must have populated the direct cache (proving the
    // second render took the hit path, not a second full blur).
    assert_eq!(raster.direct.map.len(), 1);
    assert_eq!(first.data(), second.data());
}
