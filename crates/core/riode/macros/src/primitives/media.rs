// Single responsibility: Macro expansion descriptors for raster `<img>` and vector `<svg>` media primitives.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::NodeAttribute;
use crate::engine::{node_name_string, EngineContext};
use super::attr::{attr_method, attr_value};

fn expand_media(cx: EngineContext, ctor: TokenStream2, tag: &str) -> syn::Result<TokenStream2> {
    let mut src_val: Option<TokenStream2> = None;
    let mut modifiers = Vec::new();
    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if matches!(key.as_str(), "src" | "source" | "graphic") {
                src_val = Some(attr_value(&key, attr));
            } else {
                let method = format_ident!("{}", attr_method(tag, &key));
                let val = attr_value(&key, attr);
                modifiers.push(quote! { .#method(#val) });
            }
        }
    }
    let src = src_val.ok_or_else(|| syn::Error::new_spanned(cx.el.name(), format!("view!: <{tag}> requires `src=`")))?;
    Ok(quote! { #ctor(#src) #(#modifiers)* })
}

pub fn expand_img(cx: EngineContext) -> syn::Result<TokenStream2> {
    expand_media(cx, quote! { ::riode::internal::image }, "img")
}

pub fn expand_svg(cx: EngineContext) -> syn::Result<TokenStream2> {
    expand_media(cx, quote! { ::riode::internal::svg }, "svg")
}
