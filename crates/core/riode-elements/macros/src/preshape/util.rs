// Responsibility: Build-time asset path resolution, token formatting, and diagnostic helpers.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::path::{Path, PathBuf};

pub struct BakedAsset {
    pub tokens: TokenStream2,
}

pub fn resolve_asset_path(path: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let manifest_path = Path::new(&manifest);

    // 1. Direct path relative to crate manifest
    let direct = manifest_path.join(path);
    if direct.exists() {
        return direct;
    }

    // 2. Automatic "assets/" prefix fallback if omitted
    let with_assets = manifest_path.join("assets").join(path);
    if with_assets.exists() {
        return with_assets;
    }

    // 3. Workspace parent directory search (e.g., example running in subcrate)
    if let Some(parent) = manifest_path.parent() {
        let parent_assets = parent.join("assets").join(path);
        if parent_assets.exists() {
            return parent_assets;
        }
        let parent_direct = parent.join(path);
        if parent_direct.exists() {
            return parent_direct;
        }
    }

    direct
}

pub fn warn_missing_asset(macro_name: &str, path: &str, full_path: &Path, err: &std::io::Error) {
    let msg = format!("{macro_name}!: asset `{path}` not found at {}: {err}", full_path.display());
    proc_macro::Diagnostic::new(proc_macro::Level::Warning, &msg).emit();
    eprintln!("warning: {msg}");
}

pub fn empty_image_source(w: u32, h: u32) -> TokenStream2 {
    let zeros = vec![0u8; (w * h * 4) as usize];
    let lit = syn::LitByteStr::new(&zeros, proc_macro2::Span::call_site());
    quote!(::riode::scene::ImageSource::from_static_rgba8(#w, #h, #lit))
}

pub fn bytes_literal(bytes: &[u8]) -> TokenStream2 {
    let lit = syn::LitByteStr::new(bytes, proc_macro2::Span::call_site());
    quote!(#lit)
}
