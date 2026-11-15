// Responsibility: Host-agnostic RSX compilation engine, token stream tree lowering, and primitive dispatch.

pub mod ast;
pub mod attr_node;
pub mod children;
pub mod flow;
pub mod flow_util;
pub mod for_node;
pub mod generic;
pub mod if_node;
pub mod match_node;
pub mod nested;
pub mod props;

pub use ast::{element_expr, node_name_string, EngineContext};
pub use children::{children_segments_vec, is_control_child};
pub use nested::nested_element_value;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rstml::node::Node;

pub fn view_expand(tokens: TokenStream2) -> syn::Result<TokenStream2> {
    let mut flow = flow::FlowTables::default();
    let rewritten = flow.extract(tokens);
    flow.warn();
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        out = quote! { #out #t; };
    }
    match nodes.as_slice() {
        [] => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: expected at least one element",
        )),
        [single] => {
            if is_control_child(single) {
                return Err(syn::Error::new_spanned(
                    single,
                    "view!: control nodes are not layout roots — wrap in a `<g>`",
                ));
            }
            if matches!(single, Node::Fragment(_)) {
                return Err(syn::Error::new_spanned(
                    single,
                    "view!: fragment at root — wrap in a `<g>`",
                ));
            }

            let expr = match single {
                Node::Element(el) => element_expr(el, &flow)?,
                Node::Text(t) => {
                    let s = t.value_string();
                    quote! { ::riode::internal::IntoElement::into_element(text::build().child(#s)) }
                }
                Node::Block(b) => {
                    let inner = match b {
                        rstml::node::NodeBlock::ValidBlock(block) => {
                            crate::primitives::attr::strip_braces(block)
                        }
                        other => quote! { #other },
                    };
                    quote! { ::riode::internal::IntoElement::into_element(#inner) }
                }
                _ => return Err(syn::Error::new_spanned(single, "view!: unsupported root node")),
            };

            let body = flow.splice(expr)?;
            Ok(quote! {
                {
                    #out #body
                }
            })
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: multiple roots — wrap in a `<g>`",
        )),
    }
}
