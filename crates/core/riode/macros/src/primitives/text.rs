// Responsibility: Macro expansion descriptor for the `<text>` / `<Text>` typography primitive.

use super::attr::{attr_method, attr_value_tokens, strip_braces};
use crate::engine::{is_control_child, node_name_string, EngineContext};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, Node, NodeAttribute};

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let mut content_attr: Option<TokenStream2> = None;
    let mut modifiers: Vec<TokenStream2> = Vec::new();

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if key == "content" {
                content_attr = Some(attr_value_tokens(&attr));
            } else {
                let method = format_ident!("{}", attr_method("text", &key));
                if matches!(attr.possible_value, KeyedAttributeValue::None) {
                    match key.as_str() {
                        "underline" | "italic" | "strikethrough" | "overline" => {
                            modifiers.push(quote! { .#method() });
                        }
                        _ => {
                            modifiers.push(quote! { .#method(true) });
                        }
                    }
                } else if key == "line_height" || key == "line-height" || key == "lh" {
                    let val = expand_line_height_val(attr_value_tokens(&attr));
                    modifiers.push(quote! { .#method(#val) });
                } else {
                    let val = attr_value_tokens(&attr);
                    modifiers.push(quote! { .#method(#val) });
                }
            }
        }
    }

    let child_content: Option<TokenStream2> = match cx.el.children.as_slice() {
        [] => None,
        [single] => match single {
            Node::Text(t) => {
                let s = t.value_string();
                Some(quote! { #s })
            }
            Node::Block(b) => {
                if is_control_child(single) {
                    return Err(syn::Error::new_spanned(
                        single,
                        "view!: <text> cannot branch directly",
                    ));
                }
                Some(match b {
                    rstml::node::NodeBlock::ValidBlock(blk) => strip_braces(&blk),
                    other => quote! { #other },
                })
            }
            _ => return Err(syn::Error::new_spanned(
                single,
                "view!: <text> accepts a string or expr child",
            )),
        },
        _ => return Err(syn::Error::new_spanned(
            &cx.el.children[1],
            "view!: <text> takes a single child",
        )),
    };

    let content = match (content_attr, child_content) {
        (Some(_), Some(_)) => return Err(syn::Error::new_spanned(
            cx.el.name(),
            "view!: <text> cannot have both `content=` and children",
        )),
        (Some(c), None) | (None, Some(c)) => c,
        (None, None) => return Err(syn::Error::new_spanned(
            cx.el.name(),
            "view!: <text> requires content",
        )),
    };

    Ok(quote! { ::riode::internal::text(#content) #(#modifiers)* })
}

fn expand_line_height_val(raw: TokenStream2) -> TokenStream2 {
    if let Ok(lit) = syn::parse2::<syn::LitInt>(raw.clone()) {
        if let Ok(val) = lit.base10_parse::<f32>() {
            return quote! { ::riode::prelude::LineHeight::Absolute(#val) };
        }
    }
    if let Ok(lit) = syn::parse2::<syn::LitFloat>(raw.clone()) {
        if let Ok(val) = lit.base10_parse::<f32>() {
            return quote! { ::riode::prelude::LineHeight::Absolute(#val) };
        }
    }
    if let Ok(lit) = syn::parse2::<syn::LitStr>(raw.clone()) {
        let s = lit.value();
        let s_trim = s.trim();
        if let Some(pct) = s_trim.strip_suffix('%') {
            if let Ok(val) = pct.trim().parse::<f32>() {
                let mult = val / 100.0;
                return quote! { ::riode::prelude::LineHeight::Multiple(#mult) };
            }
        }
        if let Some(px) = s_trim.strip_suffix("px") {
            if let Ok(val) = px.trim().parse::<f32>() {
                return quote! { ::riode::prelude::LineHeight::Absolute(#val) };
            }
        }
        if let Ok(val) = s_trim.parse::<f32>() {
            return quote! { ::riode::prelude::LineHeight::Multiple(#val) };
        }
    }

    quote! { ::std::convert::Into::<::riode::prelude::LineHeight>::into(#raw) }
}
