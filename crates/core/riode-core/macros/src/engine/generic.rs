// Responsibility: Uniform lowering for every lowercase tag: `tag::build()` plus
// attribute setters and `.child(...)` calls. The macro knows no tag names —
// resolution happens at the call site, so built-ins and third-party elements
// travel the same path.

use super::ast::{element_expr, node_name_string, EngineContext};
use super::children::is_control_child;
use crate::primitives::attr::{attr_method, attr_value, strip_braces};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, Node, NodeAttribute};
use syn::spanned::Spanned;

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let tag = node_name_string(cx.el.name());
    let tag_ident = format_ident!("{}", tag, span = cx.el.name().span());
    let mut out = quote! { #tag_ident::build() };

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            let method = format_ident!("{}", attr_method(&key));
            let val = match matches!(attr.possible_value, KeyedAttributeValue::None) {
                true => quote! { true },
                false => attr_value(&key, attr),
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
