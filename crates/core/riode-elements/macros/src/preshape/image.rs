// Responsibility: Macro AST lowering registering ImageTask into .maiodb with dependency anchors.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse::Parser, punctuated::Punctuated, Lit, Token};

use maio::Maiodb;
use maio_core::{AssetSource, AssetTask, TaskSpec};
use serde::{Deserialize, Serialize};

use super::util::{empty_image_source, resolve_asset_path, warn_missing_asset, BakedAsset};

const IMAGE_KIND: &str = "image:rasterize";

#[derive(Serialize, Deserialize)]
struct MacroImageTask {
    width: u32,
    height: u32,
}

impl TaskSpec for MacroImageTask {
    const KIND: &'static str = IMAGE_KIND;
}

pub fn bake_image(
    rel_path: &str,
    target_w: Option<f32>,
    target_h: Option<f32>,
) -> syn::Result<BakedAsset> {
    let raw_path = resolve_asset_path(rel_path);
    let full_path = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);

    let file_bytes = match std::fs::read(&full_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            warn_missing_asset("bake_image", rel_path, &full_path, &err);
            let w = target_w.unwrap_or(1.0).round() as u32;
            let h = target_h.unwrap_or(1.0).round() as u32;
            return Ok(BakedAsset { tokens: empty_image_source(w.max(1), h.max(1)) });
        }
    };

    let (orig_w, orig_h) = match image::ImageReader::new(std::io::Cursor::new(&file_bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())
        .and_then(|r| r.into_dimensions().map_err(|e| e.to_string()))
    {
        Ok((w, h)) => (w as f32, h as f32),
        Err(err) => {
            warn_missing_asset("bake_image", rel_path, &full_path, &std::io::Error::new(std::io::ErrorKind::InvalidData, err));
            (1.0, 1.0)
        }
    };

    let mut content_hash = [0u8; 16];
    content_hash.copy_from_slice(&blake3::hash(&file_bytes).as_bytes()[..16]);

    let (pw, ph) = calculate_dimensions(orig_w, orig_h, target_w, target_h);

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let db = Maiodb::open_workspace(std::path::Path::new(&manifest_dir))
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), format!("Failed to open workspace maiodb: {e}")))?;

    let callsite = format!("{}:{}", rel_path, pw);
    let source = AssetSource::Path { path: full_path.clone(), content_hash };
    let task = AssetTask::new(source, &MacroImageTask { width: pw, height: ph }, Some(callsite))
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), format!("Task encoding failed: {e}")))?;

    let asset_id = db.register(&task).map_err(|e| {
        syn::Error::new(proc_macro2::Span::call_site(), format!("maio registration failed: {e}"))
    })?;

    let id_bytes = asset_id.as_bytes();
    let path_str = full_path.to_string_lossy();

    let tokens = quote! {
        {
            const _: () = { let _ = include_bytes!(#path_str); };
            ::riode::image::load_asset(
                [#(#id_bytes),*],
                #pw,
                #ph,
                ::std::path::Path::new("target/assets"),
            )
        }
    };

    Ok(BakedAsset { tokens })
}

fn calculate_dimensions(orig_w: f32, orig_h: f32, tw: Option<f32>, th: Option<f32>) -> (u32, u32) {
    let (mut lw, mut lh) = match (tw, th) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w * orig_h / orig_w).round()),
        (None, Some(h)) => (((h * orig_w / orig_h).round()), h),
        (None, None) => (orig_w, orig_h),
    };

    const MAX_TEX: f32 = 2048.0;
    if lw > MAX_TEX || lh > MAX_TEX {
        let factor = (MAX_TEX / lw).min(MAX_TEX / lh);
        lw = (lw * factor).round();
        lh = (lh * factor).round();
    }

    ((lw.round() as u32).max(1), (lh.round() as u32).max(1))
}

pub fn include_image_impl(input: proc_macro::TokenStream) -> syn::Result<TokenStream2> {
    let parser = Punctuated::<Lit, Token![,]>::parse_terminated;
    let args: Vec<Lit> = parser.parse(input.into())?.into_iter().collect();
    if args.len() != 3 {
        return Err(syn::Error::new(proc_macro2::Span::call_site(), "expected include_image!(path, w, h)"));
    }
    let path = match &args[0] {
        Lit::Str(s) => s.value(),
        _ => return Err(syn::Error::new_spanned(&args[0], "first arg must be path string")),
    };
    let int_arg = |lit: &Lit, what: &str| -> syn::Result<u32> {
        match lit {
            Lit::Int(v) => v.base10_parse::<u32>(),
            _ => Err(syn::Error::new_spanned(lit, format!("{what} must be positive integer"))),
        }
    };
    let (w, h) = (int_arg(&args[1], "width")?, int_arg(&args[2], "height")?);
    let baked = bake_image(&path, Some(w as f32), Some(h as f32))?;
    Ok(baked.tokens)
}
