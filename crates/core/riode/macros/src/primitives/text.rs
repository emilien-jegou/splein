// Single responsibility: Macro expansion descriptor for the `<text>` / `<Text>` typography primitive.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{KeyedAttributeValue, Node, NodeAttribute};
use crate::engine::{is_control_child, node_name_string, EngineContext};
use super::attr::{attr_method, attr_value_tokens, slot_literal_value, strip_braces};

pub fn expand(cx: EngineContext) -> syn::Result<TokenStream2> {
    let mut content_attr: Option<TokenStream2> = None;
    let mut modifiers: Vec<TokenStream2> = Vec::new();

    for a in cx.el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if key == "content" {
                let val = attr_value_tokens(&attr);
                if let Ok(s) = syn::parse2::<syn::LitStr>(val.clone()) {
                    let idx = cx.flow.slots.get();
                    cx.flow.slots.set(idx + 1);
                    content_attr = Some(quote! {
                        ::riode::hot_reload::template::tpl_string(::std::concat!(::std::file!(), ":", ::std::line!(), ":", #idx), #s)
                    });
                } else {
                    content_attr = Some(val);
                }
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
                } else {
                    let val = slot_literal_value(attr_value_tokens(&attr), cx.flow);
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
                let idx = cx.flow.slots.get();
                cx.flow.slots.set(idx + 1);
                Some(quote! {
                    ::riode::hot_reload::template::tpl_string(::std::concat!(::std::file!(), ":", ::std::line!(), ":", #idx), #s)
                })
            }
            Node::Block(b) => {
                if is_control_child(single) {
                    return Err(syn::Error::new_spanned(single, "view!: <text> cannot branch directly"));
                }
                Some(match b {
                    rstml::node::NodeBlock::ValidBlock(blk) => strip_braces(&blk),
                    other => quote! { #other },
                })
            }
            _ => return Err(syn::Error::new_spanned(single, "view!: <text> accepts a string or expr child")),
        },
        _ => return Err(syn::Error::new_spanned(&cx.el.children[1], "view!: <text> takes a single child")),
    };

    let content = match (content_attr, child_content) {
        (Some(_), Some(_)) => return Err(syn::Error::new_spanned(cx.el.name(), "view!: <text> cannot have both `content=` and children")),
        (Some(c), None) | (None, Some(c)) => c,
        (None, None) => return Err(syn::Error::new_spanned(cx.el.name(), "view!: <text> requires content")),
    };

    Ok(quote! { ::riode::internal::text(#content) #(#modifiers)* })
}
