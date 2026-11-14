// Responsibility: Build-time typography layout, Parley shaping, and Swash glyph rasterization.

use parley::fontique::{Blob, Collection, CollectionOptions};
use parley::{
    FontContext, FontFamily, FontStyle as ParleyStyle, FontWeight, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty, TextWrapMode,
};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;
use syn::{parse::Parser, punctuated::Punctuated, Expr, Lit, MetaNameValue, Token};

use super::util::{bytes_literal, resolve_asset_path};

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

fn encode_blob(
    size: f32,
    line_height: f32,
    total_w: f32,
    total_h: f32,
    lines: &[(Vec<(u32, f32, f32, f32, usize, i32, i32, u32, u32, Vec<u8>)>, f32, f32, f32, f32)],
) -> Vec<u8> {
    fn u32le(v: u32, out: &mut Vec<u8>) { out.extend_from_slice(&v.to_le_bytes()); }
    fn f32le(v: f32, out: &mut Vec<u8>) { out.extend_from_slice(&v.to_bits().to_le_bytes()); }
    let mut out = Vec::new();
    u32le(0x41555250, &mut out);
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

pub fn pre_shaped_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let args = named_args(input)?;
    let font_path = args.get("font").map(lit_str).transpose()?.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `font = \"...\"`"))?;
    let text = args.get("text").map(lit_str).transpose()?.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `text = \"...\"`"))?;
    let size = args.get("size").map(lit_f32).transpose()?.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `size = ...`"))?;
    let line_height = args.get("line_height").map(lit_f32).transpose()?.unwrap_or(size * 1.2);

    let full_path = resolve_asset_path(&font_path);
    let font_bytes = std::fs::read(&full_path).map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), format!("cannot read font at {}: {e}", full_path.display())))?;
    let blob = Blob::from(font_bytes.clone());
    let mut collection = Collection::new(CollectionOptions { shared: false, system_fonts: false });
    collection.register_fonts(blob, None);
    let first_family = collection.family_names().next().map(|n| n.to_string());
    let mut font_cx = FontContext { collection, source_cache: Default::default() };
    let mut layout_cx = LayoutContext::new();
    let mut scaler = swash::scale::ScaleContext::new();

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
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue; };
                let run = glyph_run.run();
                let starts = run_clusters.get(&(run.index() as u32)).map_or(&[][..], Vec::as_slice);
                let mut pos = 0usize;
                for g in glyph_run.positioned_glyphs() {
                    let cluster = starts.get(pos).copied().unwrap_or(0);
                    pos += 1;
                    let font = swash::FontRef::from_index(&font_bytes, run.font().index as usize).ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "font face unavailable"))?;
                    let mut context = scaler.builder(font).size(size).hint(size <= 16.0).normalized_coords(&Vec::<i16>::new()).build();
                    let render = swash::scale::Render::new(&[swash::scale::Source::ColorOutline(0), swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit), swash::scale::Source::Outline]);
                    let image = render.render(&mut context, g.id as u16);
                    let (left, top, w, h, data) = match image {
                        Some(ref img) if img.content == swash::scale::image::Content::Mask => {
                            (img.placement.left, img.placement.top, img.placement.width, img.placement.height, img.data.clone())
                        }
                        _ => (0, 0, 0, 0, Vec::new()),
                    };
                    glyphs.push((g.id as u32, g.x, line_top + g.y, g.advance, cluster, left, top, w, h, data));
                }
            }
            let mut width = 0.0f32;
            for g in glyphs.iter() { width = width.max(g.1 + g.3); }
            lines_out.push((std::mem::take(&mut glyphs), width, baseline));
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
    Ok(quote!(::riode::text::PreShapedText::from_bytes(#lit).unwrap()))
}
