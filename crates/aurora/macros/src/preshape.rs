// Single responsibility: Build-time typography + vector pre-computation macros.
//
// `pre_shaped!` shapes static text on the build host (parley) and rasterizes
// coverage bitmaps (swash) at integer pen phase, emitting a versioned byte
// blob the runtime rehydrates with zero shaping latency and zero font
// lookups (`TextContext::shape_pre_shaped`). `include_svg!` rasterizes an SVG
// file on the host (resvg) and emits an `ImageSource` constructor call.
//
// Mirror notes (divergence risk): the shaping subset below mirrors
// `aurora::text::{shape,runs}` (single-line, start-aligned, no wrap/balance,
// no ellipsis) and the raster mirrors `TextContext::raster_inner` at phase 0
// with the same hint cutoff (16px). The metrics-equality test
// (`tests/preshaped_preshape.rs`) shapes the same strings both ways and
// fails on drift — run it after touching either side.
//
// These macros run on the build host (`std` + fs access via
// `CARGO_MANIFEST_DIR`) and emit plain byte literals: no font/SVG code
// ships in the target binary for covered content.

use parley::fontique::{Blob, Collection, CollectionOptions};
use parley::{
    FontContext, FontFamily, FontStyle as ParleyStyle, FontWeight, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty, TextWrapMode,
};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;
use syn::{parse::Parser, punctuated::Punctuated, Expr, Lit, MetaNameValue, Token};

/// Parsed `name = value` macro arguments.
fn named_args(input: proc_macro::TokenStream) -> syn::Result<HashMap<String, Expr>> {
    let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
    let pairs = parser.parse(input.into())?;
    let mut map = HashMap::new();
    for p in pairs {
        map.insert(p.path.require_ident()?.to_string(), p.value);
    }
    Ok(map)
}

fn lit_str(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Lit(e) => match &e.lit {
            Lit::Str(s) => Ok(s.value()),
            _ => Err(syn::Error::new_spanned(expr, "expected string literal")),
        },
        _ => Err(syn::Error::new_spanned(expr, "expected string literal")),
    }
}

fn lit_f32(expr: &Expr) -> syn::Result<f32> {
    match expr {
        Expr::Lit(e) => match &e.lit {
            Lit::Float(f) => f.base10_parse::<f32>(),
            Lit::Int(i) => Ok(i.base10_parse::<u32>()? as f32),
            _ => Err(syn::Error::new_spanned(expr, "expected number literal")),
        },
        _ => Err(syn::Error::new_spanned(expr, "expected number literal")),
    }
}

fn manifest_relative(path: &str) -> std::path::PathBuf {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&dir).join(path)
}

/// Byte-blob encoder mirroring `aurora::text::preshaped::encode_blob`
/// (duplicated: the macro crate cannot depend on aurora — cycle).
#[allow(clippy::too_many_arguments)]
fn encode_blob(
    size: f32,
    line_height: f32,
    total_w: f32,
    total_h: f32,
    lines: &[(
        Vec<(u32, f32, f32, f32, usize, i32, i32, u32, u32, Vec<u8>)>,
        f32,
        f32,
        f32,
        f32,
    )],
) -> Vec<u8> {
    fn u32le(v: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn f32le(v: f32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    let mut out = Vec::new();
    u32le(0x41555250, &mut out); // "AURP"
    u32le(1, &mut out);
    f32le(size, &mut out);
    f32le(line_height, &mut out);
    f32le(total_w, &mut out);
    f32le(total_h, &mut out);
    u32le(lines.len() as u32, &mut out);
    for (glyphs, width, height, baseline, font_size) in lines {
        u32le(glyphs.len() as u32, &mut out);
        f32le(*width, &mut out);
        f32le(*height, &mut out);
        f32le(*baseline, &mut out);
        f32le(*font_size, &mut out);
        for (id, x, y, adv, cluster, left, top, w, h, data) in glyphs {
            u32le(*id, &mut out);
            f32le(x.round(), &mut out);
            f32le(y.round(), &mut out);
            f32le(*adv, &mut out);
            u32le(*cluster as u32, &mut out);
            u32le(*left as u32, &mut out);
            u32le(*top as u32, &mut out);
            u32le(*w, &mut out);
            u32le(*h, &mut out);
            out.extend_from_slice(data);
        }
    }
    out
}

fn bytes_literal(bytes: &[u8]) -> TokenStream2 {
    let els = bytes.iter().map(|b| quote!(#b));
    quote!(&[#(#els),*])
}

/// `pre_shaped!(font = "path.ttf", text = "...", size = 16.0[, line_height = 20.0])`
///
/// Shapes one static string at build time. Single-line rhythm per physical
/// line (split on `\n`), start-aligned, no wrap/balance/ellipsis — wrapped
/// or aligned text stays on the runtime path. Emits
/// `::aurora::text::PreShapedText::from_bytes(&[...]).unwrap()`.
pub fn pre_shaped_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let args = named_args(input)?;
    let font_path = args
        .get("font")
        .map(lit_str)
        .transpose()?
        .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `font = \"...\"`"))?;
    let text = args
        .get("text")
        .map(lit_str)
        .transpose()?
        .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `text = \"...\"`"))?;
    let size = args
        .get("size")
        .map(lit_f32)
        .transpose()?
        .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `size = ...`"))?;
    if !(size.is_finite() && size > 0.0) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "size must be a positive finite number",
        ));
    }
    let line_height = args
        .get("line_height")
        .map(lit_f32)
        .transpose()?
        .unwrap_or(size * 1.2);
    if !(line_height.is_finite() && line_height > 0.0) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "line_height must be a positive finite number",
        ));
    }

    let font_bytes = std::fs::read(manifest_relative(&font_path))
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), format!("cannot read font: {e}")))?;
    let blob = Blob::from(font_bytes.clone());
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    collection.register_fonts(blob, None);
    let first_family: Option<String> = collection
        .family_names()
        .next()
        .map(|n| n.to_string());
    let mut font_cx = FontContext {
        collection,
        source_cache: Default::default(),
    };
    let mut layout_cx = LayoutContext::new();
    let mut scaler = swash::scale::ScaleContext::new();

    // Shape each physical line independently (mirrors shape_at_width with
    // wrap=None; multi-line rhythm matches runs.rs: uniform boxes stacked
    // from zero, baseline = top + metrics.baseline).
    let mut lines_out = Vec::new();
    let mut max_w = 0.0f32;
    let physical: Vec<&str> = text.split('\n').collect();
    for (line_idx, line_text) in physical.iter().enumerate() {
        let mut builder = layout_cx.ranged_builder(&mut font_cx, line_text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(line_height)));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(400.0)));
        builder.push_default(StyleProperty::FontStyle(ParleyStyle::Normal));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        if let Some(name) = first_family.as_deref() {
            builder.push_default(StyleProperty::FontFamily(FontFamily::named(name)));
        }
        let mut layout: Layout<()> = builder.build(line_text);
        layout.break_all_lines(None);

        let line_top = line_idx as f32 * line_height;
        let mut glyphs = Vec::new();
        for line in layout.lines() {
            let baseline = line_top + line.metrics().baseline;
            // Cluster byte offsets per run in visual order (mirrors runs.rs).
            let mut run_clusters: HashMap<u32, Vec<usize>> = HashMap::new();
            for run in line.runs() {
                let mut starts = Vec::new();
                for cluster in run.visual_clusters() {
                    let start = cluster.text_range().start;
                    starts.extend(cluster.glyphs().map(|_| start));
                }
                run_clusters.insert(run.index() as u32, starts);
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let starts: &[usize] = run_clusters
                    .get(&(run.index() as u32))
                    .map_or(&[], Vec::as_slice);
                let mut pos = 0usize;
                for g in glyph_run.positioned_glyphs() {
                    let cluster = starts.get(pos).copied().unwrap_or(0);
                    pos += 1;
                    // Swash raster at integer pen phase (mirrors
                    // raster_inner at phase 0 with the same hint rule).
                    let font =
                        swash::FontRef::from_index(&font_bytes, run.font().index as usize)
                            .ok_or_else(|| {
                                syn::Error::new(
                                    proc_macro2::Span::call_site(),
                                    "font face unavailable to swash",
                                )
                            })?;
                    let mut context = scaler
                        .builder(font)
                        .size(size)
                        .hint(size <= 16.0)
                        .normalized_coords(&Vec::<i16>::new())
                        .build();
                    let mut render = swash::scale::Render::new(&[
                        swash::scale::Source::ColorOutline(0),
                        swash::scale::Source::ColorBitmap(
                            swash::scale::StrikeWith::BestFit,
                        ),
                        swash::scale::Source::Outline,
                    ]);
                    let image = render.render(&mut context, g.id as u16);
                    let (left, top, w, h, data) = match image {
                        Some(ref img)
                            if img.content == swash::scale::image::Content::Mask =>
                        {
                            (
                                img.placement.left,
                                img.placement.top,
                                img.placement.width,
                                img.placement.height,
                                img.data.clone(),
                            )
                        }
                        _ => (0, 0, 0, 0, Vec::new()),
                    };
                    glyphs.push((
                        g.id as u32,
                        g.x,
                        line_top + g.y,
                        g.advance,
                        cluster,
                        left,
                        top,
                        w,
                        h,
                        data,
                    ));
                }
            }
            // Content width (mirrors line_content_width: max pen, trailing
            // whitespace trimmed; clusters here are line-local, matching the
            // split line text).
            let mut width = 0.0f32;
            for g in glyphs.iter() {
                width = width.max(g.1 + g.3);
            }
            let bytes = line_text.as_bytes();
            let mut end = width;
            for g in glyphs.iter().rev() {
                let is_ws = matches!(bytes.get(g.4), Some(b) if b.is_ascii_whitespace());
                if !is_ws {
                    break;
                }
                end = end.min(g.1);
            }
            lines_out.push((std::mem::take(&mut glyphs), end, baseline));
        }
    }
    let total_h = (physical.len() as f32 * line_height).max(line_height);
    let mut encoded_lines = Vec::new();
    for (glyphs, width, baseline) in &lines_out {
        max_w = max_w.max(*width);
        encoded_lines.push((glyphs.clone(), *width, line_height, *baseline, size));
    }
    let blob = encode_blob(size, line_height, max_w.max(0.0), total_h, &encoded_lines);
    let lit = bytes_literal(&blob);
    Ok(quote!(::aurora::text::PreShapedText::from_bytes(#lit).unwrap()))
}

/// `include_svg!("path.svg", 64, 64)`
///
/// Rasterizes the SVG on the build host at exactly `width`x`height` device
/// px and emits `::aurora::scene::ImageSource::from_static_rgba8(..)`.
/// Dynamic theming/animation stays on the runtime resvg path.
pub fn include_svg_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let parser =
        Punctuated::<Lit, Token![,]>::parse_terminated;
    let args: Vec<Lit> = parser.parse(input.into())?.into_iter().collect();
    if args.len() != 3 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected include_svg!(\"path.svg\", WIDTH, HEIGHT)",
        ));
    }
    let path = match &args[0] {
        Lit::Str(s) => s.value(),
        _ => {
            return Err(syn::Error::new_spanned(
                &args[0],
                "first argument must be the SVG path string",
            ))
        }
    };
    let int_arg = |i: usize, what: &str| -> syn::Result<u32> {
        match &args[i] {
            Lit::Int(v) => v.base10_parse::<u32>().map_err(|_| {
                syn::Error::new_spanned(&args[i], format!("{what} must be a positive integer"))
            }),
            _ => Err(syn::Error::new_spanned(
                &args[i],
                format!("{what} must be a positive integer"),
            )),
        }
    };
    let (w, h) = (int_arg(1, "width")?, int_arg(2, "height")?);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "width/height must be in 1..=4096",
        ));
    }
    let svg_data = std::fs::read(manifest_relative(&path)).map_err(|e| {
        syn::Error::new(proc_macro2::Span::call_site(), format!("cannot read SVG: {e}"))
    })?;
    let svg_str = String::from_utf8(svg_data).map_err(|_| {
        syn::Error::new(proc_macro2::Span::call_site(), "SVG file is not UTF-8")
    })?;
    let tree = resvg::usvg::Tree::from_str(&svg_str, &resvg::usvg::Options::default())
        .map_err(|e| {
            syn::Error::new(proc_macro2::Span::call_site(), format!("SVG parse failed: {e}"))
        })?;
    let tree_w = tree.size().width();
    let tree_h = tree.size().height();
    if !(tree_w > 0.0 && tree_h > 0.0) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "SVG has zero size",
        ));
    }
    let mut pixmap =
        tiny_skia::Pixmap::new(w, h).ok_or_else(|| {
            syn::Error::new(proc_macro2::Span::call_site(), "pixmap allocation failed")
        })?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(w as f32 / tree_w, h as f32 / tree_h),
        &mut pixmap.as_mut(),
    );
    let lit = bytes_literal(pixmap.data());
    Ok(quote!(::aurora::scene::ImageSource::from_static_rgba8(#w, #h, #lit)))
}
