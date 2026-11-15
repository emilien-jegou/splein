// Responsibility: Uniform lowering for every lowercase tag: `tag::build()` plus
// attribute setters and `.child(...)` calls. The macro knows no tag names —
// resolution happens at the call site, so built-ins and third-party elements
// travel the same path.
//
// One attribute-driven behavior lives here: a `src` (or `source`/`graphic`)
// whose value is a string literal expands to the `bake_src!` bridge macro
// (resolved through the facade), so build-time asset baking lives entirely in
// the elements crate. Keyed on the attribute, not the tag, so an overridden
// `img` or a third party's `video` gets the same treatment for free.

use super::ast::{element_expr, node_name_string, EngineContext};
use super::children::is_control_child;
use crate::primitives::attr::{attr_method, attr_value, strip_braces};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, Node, NodeAttribute};
use syn::spanned::Spanned;

fn extract_literal_str(attr: &rstml::node::KeyedAttribute) -> Option<String> {
    use crate::primitives::attr::attr_expr;
    match attr_expr(attr) {
        Some(syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. })) => Some(s.value()),
        _ => None,
    }
}

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let tag = node_name_string(cx.el.name());
    let tag_ident = format_ident!("{}", tag, span = cx.el.name().span());

    // Pre-scan: does any attribute carry a string-literal source? If so the
    // whole element gets the bake_src! bridge; sizing is the element's job.
    let mut src_lit: Option<String> = None;
    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if matches!(key.as_str(), "src" | "source" | "graphic") {
                if let Some(s) = extract_literal_str(attr) {
                    src_lit = Some(s);
                }
            }
        }
    }

    let mut out = quote! { #tag_ident::build() };

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            let method = format_ident!("{}", attr_method(&key));
            let val = if matches!(key.as_str(), "src" | "source" | "graphic")
                && matches!(attr.possible_value, KeyedAttributeValue::None)
            {
                quote! { true }
            } else if let Some(s) = &src_lit {
                if matches!(key.as_str(), "src" | "source" | "graphic") {
                    quote! { ::riode::bake_src!(#s) }
                } else {
                    attr_value(&key, attr)
                }
            } else {
                attr_value(&key, attr)
            };
            out = quote! { #out.#method(#val) };
        }
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
