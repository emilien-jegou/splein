// Responsibility: Macro expansion descriptor for the `<g>` / `<Group>` flex container primitive.

use super::attr::{attr_method, attr_value};
use crate::engine::{children_segments_vec, node_name_string, EngineContext};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, NodeAttribute};

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let mut out = quote! { ::riode::internal::group() };
    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            let method = format_ident!("{}", attr_method("g", &key));
            if matches!(attr.possible_value, KeyedAttributeValue::None) {
                match key.as_str() {
                    "col" | "row" | "scroll_y" | "scroll_x" | "scroll" | "absolute" => {
                        out = quote! { #out.#method() };
                    }
                    _ => {
                        out = quote! { #out.#method(true) };
                    }
                }
            } else {
                let val = attr_value(&key, attr);
                out = quote! { #out.#method(#val) };
            }
        }
    }
    if !cx.el.children.is_empty() {
        let kids = children_segments_vec(&cx.el.children, cx.flow, cx.engine)?;
        out = quote! { #out.child_segments(#kids) };
    }
    Ok(out)
}
