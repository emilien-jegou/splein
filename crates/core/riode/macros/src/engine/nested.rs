// Responsibility: Handling nested RSX blocks inside attribute positions.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rstml::node::Node;
use super::ast::{element_expr, node_name_string};
use super::flow::FlowTables;
use super::registry::RxEngine;

pub fn nested_element_value(body: &TokenStream2, engine: &RxEngine) -> TokenStream2 {
    let is_rsx = matches!(
        body.clone().into_iter().next(),
        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '<'
    );
    if !is_rsx {
        return quote! { {#body} };
    }
    let mut flow = FlowTables::default();
    let rewritten = flow.extract(body.clone());
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
        [Node::Element(el)] => {
            let tag = node_name_string(el.name());
            if matches!(tag.as_str(), "for" | "if" | "match" | "else" | "case") {
                return quote! {
                    #out ::std::compile_error!("view!: control nodes must be direct children")
                };
            }
            match element_expr(el, &flow, engine) {
                Ok(e) => {
                    let e = match flow.splice(e, engine) {
                        Ok(s) => s,
                        Err(err) => err.to_compile_error(),
                    };
                    quote! { #out (#e) }
                }
                Err(err) => {
                    let compile = err.to_compile_error();
                    quote! { #out #compile }
                }
            }
        }
        _ => quote! {
            #out ::std::compile_error!("view!: an attribute holds one nested element")
        },
    }
}
