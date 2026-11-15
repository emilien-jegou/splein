// Responsibility: Uniform lowering for every lowercase tag: `tag::build()` plus
// attribute setters and `.child(...)` calls. The macro knows no tag names —
// resolution happens at the call site, so built-ins and third-party elements
// travel the same path.
//
// One attribute-driven behavior lives here: a `src` (or `source`/`graphic`)
// whose value is a string literal bakes the file at compile time — dispatched
// by extension (`.svg` rasterized, anything else decoded) — and, when the tag
// carries no explicit `w`/`h`, injects the asset's natural bounds. This is
// keyed on the attribute, not the tag, so an overridden `img` or a third
// party's `video` gets the same treatment for free.

use super::ast::{element_expr, node_name_string, EngineContext};
use super::children::is_control_child;
use crate::preshape::{bake_image, bake_svg};
use crate::primitives::attr::{attr_method, attr_value, strip_braces};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, Node, NodeAttribute};
use syn::spanned::Spanned;
use syn::Lit;

fn extract_literal_str(attr: &rstml::node::KeyedAttribute) -> Option<String> {
    match attr_value_expr(attr) {
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. })) => Some(s.value()),
        _ => None,
    }
}

fn extract_literal_f32(attr: &rstml::node::KeyedAttribute) -> Option<f32> {
    match attr_value_expr(attr) {
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Float(f), .. })) => f.base10_parse::<f32>().ok(),
        Some(syn::Expr::Lit(syn::ExprLit { lit: Lit::Int(i), .. })) => {
            i.base10_parse::<u32>().ok().map(|v| v as f32)
        }
        _ => None,
    }
}

/// The expression an attribute carries, if any (absent for bare `src` flags).
fn attr_value_expr(attr: &rstml::node::KeyedAttribute) -> Option<syn::Expr> {
    use crate::primitives::attr::attr_expr;
    attr_expr(attr)
}

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let tag = node_name_string(cx.el.name());
    let tag_ident = format_ident!("{}", tag, span = cx.el.name().span());

    // Pre-scan: literal string source + explicit sizing, so a literal `src`
    // can bake at the declared target size (or natural size when none).
    let mut src_lit: Option<String> = None;
    let mut has_w = false;
    let mut has_h = false;
    let mut target_w: Option<f32> = None;
    let mut target_h: Option<f32> = None;
    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            match key.as_str() {
                "src" | "source" | "graphic" => {
                    if let Some(s) = extract_literal_str(attr) {
                        src_lit = Some(s);
                    }
                }
                "w" | "width" => {
                    has_w = true;
                    target_w = extract_literal_f32(attr);
                }
                "h" | "height" => {
                    has_h = true;
                    target_h = extract_literal_f32(attr);
                }
                _ => {}
            }
        }
    }

    let baked = match &src_lit {
        Some(path) => {
            let is_svg = path.to_ascii_lowercase().ends_with(".svg");
            Some(if is_svg {
                bake_svg(path, target_w, target_h)?
            } else {
                bake_image(path, target_w, target_h)?
            })
        }
        None => None,
    };

    let mut out = quote! { #tag_ident::build() };
    let mut modifiers: Vec<TokenStream2> = Vec::new();

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            let method = format_ident!("{}", attr_method(&key));
            let val = if matches!(key.as_str(), "src" | "source" | "graphic")
                && matches!(attr.possible_value, KeyedAttributeValue::None)
            {
                quote! { true }
            } else if let Some(b) = &baked {
                if matches!(key.as_str(), "src" | "source" | "graphic")
                    && src_lit.is_some()
                {
                    b.tokens.clone()
                } else {
                    attr_value(&key, attr)
                }
            } else {
                attr_value(&key, attr)
            };
            modifiers.push(quote! { .#method(#val) });
        }
    }

    // Natural bounds: a fully unconstrained media tag takes its intrinsic size
    // instead of stretching to the available width.
    if src_lit.is_some() && !has_w && !has_h {
        if let Some(b) = &baked {
            let (lw, lh) = (b.logical_w, b.logical_h);
            modifiers.push(quote! { .w(#lw).h(#lh) });
        }
    }

    for m in modifiers {
        out = quote! { #out #m };
    }

    for child in &cx.el.children {
        if is_control_child(child) {
            return Err(syn::Error::new_spanned(
                child,
                format!("view!: control nodes cannot be children of `<{tag}>`"),
            ));
        }
        let val = match child {
            Node::Text(t) => {
                let s = t.value_string();
                quote! { #s }
            }
            Node::Block(b) => match b {
                rstml::node::NodeBlock::ValidBlock(blk) => strip_braces(blk),
                other => quote! { #other },
            },
            Node::Element(el) => {
                let inner = element_expr(el, cx.flow)?;
                cx.flow.splice(inner)?
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    child,
                    format!("view!: unsupported child of `<{tag}>`"),
                ))
            }
        };
        out = quote! { #out.child(#val) };
    }

    Ok(out)
}
