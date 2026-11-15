// Responsibility: Build-time decoding, Lanczos3 resampling, and static byte baking for raster images.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse::Parser, punctuated::Punctuated, Lit, Token};

use super::util::{bytes_literal, empty_image_source, resolve_asset_path, warn_missing_asset, BakedAsset};

pub fn bake_image(
    rel_path: &str,
    target_w: Option<f32>,
    target_h: Option<f32>,
) -> syn::Result<BakedAsset> {
    let full_path = resolve_asset_path(rel_path);
    let img_data = match std::fs::read(&full_path) {
        Ok(data) => data,
        Err(err) => {
            warn_missing_asset("bake_image", rel_path, &full_path, &err);
            let w = target_w.unwrap_or(1.0).round() as u32;
            let h = target_h.unwrap_or(1.0).round() as u32;
            return Ok(BakedAsset { tokens: empty_image_source(w.max(1), h.max(1)) });
        }
    };

    let img = image::load_from_memory(&img_data).map_err(|e| {
        syn::Error::new(proc_macro2::Span::call_site(), format!("image decode failed for `{rel_path}`: {e}"))
    })?;

    let (orig_w, orig_h) = (img.width() as f32, img.height() as f32);
    let (mut lw, mut lh) = match (target_w, target_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w * orig_h / orig_w).round()),
        (None, Some(h)) => (((h * orig_w / orig_h).round()), h),
        (None, None) => (orig_w, orig_h),
    };

    // Safety guard: Clamp oversized images to 2048px to prevent GPU downlevel texture overflow
    const MAX_TEX: f32 = 2048.0;
    if lw > MAX_TEX || lh > MAX_TEX {
        let factor = (MAX_TEX / lw).min(MAX_TEX / lh);
        lw = (lw * factor).round();
        lh = (lh * factor).round();
    }

    let pw = (lw.round() as u32).max(1);
    let ph = (lh.round() as u32).max(1);

    let resized = if pw == img.width() && ph == img.height() {
        img
    } else {
        img.resize_exact(pw, ph, image::imageops::FilterType::Lanczos3)
    };

    let mut rgba = resized.into_rgba8();
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u32;
        p[0] = ((p[0] as u32 * a) / 255) as u8;
        p[1] = ((p[1] as u32 * a) / 255) as u8;
        p[2] = ((p[2] as u32 * a) / 255) as u8;
    }

    let bytes = bytes_literal(&rgba.into_raw());
    let tokens = quote!(::riode::scene::ImageSource::from_static_rgba8(#pw, #ph, #bytes));

    Ok(BakedAsset { tokens })
}

pub fn include_image_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let parser = Punctuated::<Lit, Token![,]>::parse_terminated;
    let args: Vec<Lit> = parser.parse(input.into())?.into_iter().collect();
    if args.len() != 3 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected include_image!(\"assets/path.png\", WIDTH, HEIGHT)",
        ));
    }
    let path = match &args[0] {
        Lit::Str(s) => s.value(),
        _ => return Err(syn::Error::new_spanned(&args[0], "first argument must be the image path string")),
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
    let baked = bake_image(&path, Some(w as f32), Some(h as f32))?;
    Ok(baked.tokens)
}
