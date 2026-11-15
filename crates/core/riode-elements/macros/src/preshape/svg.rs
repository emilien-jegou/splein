// Responsibility: Build-time vector graphic parsing, scale rasterization, and static byte baking for SVGs.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse::Parser, punctuated::Punctuated, Lit, Token};

use super::image::bake_image;
use super::util::{bytes_literal, empty_image_source, resolve_asset_path, warn_missing_asset, BakedAsset};

pub fn bake_svg(
    rel_path: &str,
    target_w: Option<f32>,
    target_h: Option<f32>,
) -> syn::Result<BakedAsset> {
    let full_path = resolve_asset_path(rel_path);
    let svg_data = match std::fs::read(&full_path) {
        Ok(data) => data,
        Err(err) => {
            warn_missing_asset("bake_svg", rel_path, &full_path, &err);
            let w = target_w.unwrap_or(1.0).round() as u32;
            let h = target_h.unwrap_or(1.0).round() as u32;
            return Ok(BakedAsset { tokens: empty_image_source(w.max(1), h.max(1)) });
        }
    };

    let svg_str = String::from_utf8(svg_data).map_err(|_| {
        syn::Error::new(proc_macro2::Span::call_site(), format!("SVG `{rel_path}` is not UTF-8"))
    })?;

    let tree = resvg::usvg::Tree::from_str(&svg_str, &resvg::usvg::Options::default()).map_err(|e| {
        syn::Error::new(proc_macro2::Span::call_site(), format!("SVG parse failed for `{rel_path}`: {e}"))
    })?;

    let orig_w = tree.size().width();
    let orig_h = tree.size().height();
    if !(orig_w > 0.0 && orig_h > 0.0) {
        return Err(syn::Error::new(proc_macro2::Span::call_site(), format!("SVG `{rel_path}` has zero size")));
    }

    let (lw, lh) = match (target_w, target_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w * orig_h / orig_w).ceil()),
        (None, Some(h)) => (((h * orig_w / orig_h).ceil()), h),
        (None, None) => (orig_w, orig_h),
    };

    let pw = (lw.round() as u32).max(1);
    let ph = (lh.round() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or_else(|| {
        syn::Error::new(proc_macro2::Span::call_site(), "SVG pixmap allocation failed")
    })?;

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(pw as f32 / orig_w, ph as f32 / orig_h),
        &mut pixmap.as_mut(),
    );

    let bytes = bytes_literal(pixmap.data());
    let tokens = quote!(::riode::scene::ImageSource::from_static_rgba8(#pw, #ph, #bytes));

    Ok(BakedAsset { tokens })
}

pub fn include_svg_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let parser = Punctuated::<Lit, Token![,]>::parse_terminated;
    let args: Vec<Lit> = parser.parse(input.into())?.into_iter().collect();
    if args.len() != 3 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected include_svg!(\"assets/path.svg\", WIDTH, HEIGHT)",
        ));
    }
    let path = match &args[0] {
        Lit::Str(s) => s.value(),
        _ => return Err(syn::Error::new_spanned(&args[0], "first argument must be the SVG path string")),
    };
    let int_arg = |lit: &Lit, what: &str| -> syn::Result<u32> {
        match lit {
            Lit::Int(v) => v.base10_parse::<u32>().map_err(|_| {
                syn::Error::new_spanned(lit, format!("{what} must be a positive integer"))
            }),
            _ => Err(syn::Error::new_spanned(lit, format!("{what} must be a positive integer"))),
        }
    };
    let (w, h) = (int_arg(&args[1], "width")?, int_arg(&args[2], "height")?);
    let baked = bake_svg(&path, Some(w as f32), Some(h as f32))?;
    Ok(baked.tokens)
}

/// `bake_src!("path")` / `bake_src!("path", w, h)` — the string-literal `src=`
/// bridge emitted by the generic `view!` lowering. Extension-dispatched:
/// `.svg` rasterizes, anything else decodes. Target dims are optional and
/// only forwarded when the element declared literal `w` *and* `h`.
pub fn bake_src_impl(tokens: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let parser = |input: syn::parse::ParseStream| -> syn::Result<(syn::LitStr, Option<f32>, Option<f32>)> {
        let path: syn::LitStr = input.parse()?;
        let mut target_w = None;
        let mut target_h = None;
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            target_w = Some(input.parse::<syn::LitFloat>()?.base10_parse::<f32>()?);
            input.parse::<syn::Token![,]>()?;
            target_h = Some(input.parse::<syn::LitFloat>()?.base10_parse::<f32>()?);
        }
        Ok((path, target_w, target_h))
    };
    let (path, target_w, target_h) = syn::parse::Parser::parse2(parser, tokens.into())?;

    let baked = if path.value().to_ascii_lowercase().ends_with(".svg") {
        bake_svg(&path.value(), target_w, target_h)?
    } else {
        bake_image(&path.value(), target_w, target_h)?
    };
    Ok(baked.tokens)
}
