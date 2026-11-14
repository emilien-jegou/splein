// Responsibility: Macro expansion descriptors for raster `<img>` and vector `<svg>` media primitives.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttribute, NodeAttribute};
use syn::Lit;

use crate::engine::{node_name_string, EngineContext};
use crate::preshape::{bake_image, bake_svg};
use super::attr::{attr_expr, attr_method, attr_value};

fn extract_literal_str(attr: &KeyedAttribute) -> Option<String> {
    match attr_expr(attr) {
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. })) => Some(s.value()),
        _ => None,
    }
}

fn extract_literal_f32(attr: &KeyedAttribute) -> Option<f32> {
    match attr_expr(attr) {
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Float(f), .. })) => f.base10_parse::<f32>().ok(),
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Int(i), .. })) => {
            i.base10_parse::<u32>().ok().map(|v| v as f32)
        }
        _ => None,
    }
}

fn expand_media_with_bake(
    cx: EngineContext,
    ctor: TokenStream2,
    tag: &str,
    is_svg: bool,
) -> syn::Result<TokenStream2> {
    let mut raw_src_lit: Option<String> = None;
    let mut dynamic_src: Option<TokenStream2> = None;
    let mut target_w: Option<f32> = None;
    let mut target_h: Option<f32> = None;
    let mut has_w = false;
    let mut has_h = false;
    let mut modifiers = Vec::new();

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            match key.as_str() {
                "src" | "source" | "graphic" => {
                    if let Some(s) = extract_literal_str(attr) {
                        raw_src_lit = Some(s);
                    } else {
                        dynamic_src = Some(attr_value(&key, attr));
                    }
                }
                "w" | "width" => {
                    has_w = true;
                    target_w = extract_literal_f32(attr);
                    let method = format_ident!("{}", attr_method(tag, &key));
                    let val = attr_value(&key, attr);
                    modifiers.push(quote! { .#method(#val) });
                }
                "h" | "height" => {
                    has_h = true;
                    target_h = extract_literal_f32(attr);
                    let method = format_ident!("{}", attr_method(tag, &key));
                    let val = attr_value(&key, attr);
                    modifiers.push(quote! { .#method(#val) });
                }
                _ => {
                    let method = format_ident!("{}", attr_method(tag, &key));
                    let val = attr_value(&key, attr);
                    modifiers.push(quote! { .#method(#val) });
                }
            }
        }
    }

    let src = if let Some(path) = raw_src_lit {
        let baked = if is_svg {
            bake_svg(&path, target_w, target_h)?
        } else {
            bake_image(&path, target_w, target_h)?
        };

        // For SVGs (icons), inject natural bounds if completely unconstrained.
        // For images without explicit w/h, provide natural fallback bounds.
        if is_svg && !has_w && !has_h {
            let lw = baked.logical_w;
            let lh = baked.logical_h;
            modifiers.push(quote! { .w(#lw).h(#lh) });
        } else if !is_svg && !has_w && !has_h {
            let lw = baked.logical_w;
            let lh = baked.logical_h;
            modifiers.push(quote! { .w(#lw).h(#lh) });
        }

        baked.tokens
    } else if let Some(expr) = dynamic_src {
        expr
    } else {
        return Err(syn::Error::new_spanned(
            cx.el.name(),
            format!("view!: <{tag}> requires `src=`"),
        ));
    };

    Ok(quote! { #ctor(#src) #(#modifiers)* })
}

pub fn expand_img(cx: EngineContext) -> syn::Result<TokenStream2> {
    expand_media_with_bake(cx, quote! { ::riode::internal::image }, "img", false)
}

pub fn expand_svg(cx: EngineContext) -> syn::Result<TokenStream2> {
    expand_media_with_bake(cx, quote! { ::riode::internal::svg }, "svg", true)
}
