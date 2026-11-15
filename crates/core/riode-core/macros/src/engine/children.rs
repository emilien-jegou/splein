// Responsibility: Recursive lowering of RSX child lists and control-flow splices.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rstml::node::Node;
use super::ast::{element_expr, ViewNode};
use super::flow::FlowTables;


pub fn expand_children_vec(tokens: TokenStream2) -> syn::Result<TokenStream2> {
    let mut flow = FlowTables::default();
    let rewritten = flow.extract(tokens);
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut error_out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        error_out = quote! { #error_out #t; };
    }
    let mut items = Vec::new();
    for c in &nodes {
        items.push(child_segment(c, &flow)?);
    }
    Ok(quote! { { #error_out ::std::vec![#(#items),*] } })
}

pub fn children_vec(children: &[ViewNode], flow: &FlowTables) -> syn::Result<TokenStream2> {
    let mut items = Vec::new();
    for c in children {
        items.push(child_expr(c, flow)?);
    }
    Ok(quote! {
        {
            let mut __riode_kids: ::std::vec::Vec<::riode::internal::Element> = ::std::vec::Vec::new();
            #(::riode::internal::IntoSplice::splice_into(#items, &mut __riode_kids);)*
            __riode_kids
        }
    })
}

pub fn children_segments_vec(children: &[ViewNode], flow: &FlowTables) -> syn::Result<TokenStream2> {
    let mut items = Vec::new();
    for c in children {
        items.push(child_segment(c, flow)?);
    }
    Ok(quote! { ::std::vec![#(#items),*] })
}


pub fn child_segment(node: &ViewNode, flow: &FlowTables) -> syn::Result<TokenStream2> {
    if is_control_child(node) {
        let ident_ts: TokenStream2 = match node {
            Node::Block(rstml::node::NodeBlock::ValidBlock(b)) => quote! { #b },
            _ => unreachable!(),
        };
        return flow.splice(ident_ts);
    }
    let e = child_expr(node, flow)?;
    let e = flow.splice(e)?;
    Ok(quote! { ::riode::internal::ChildSegment::Static(#e) })
}

pub fn child_expr(node: &ViewNode, flow: &FlowTables) -> syn::Result<TokenStream2> {
    match node {
        Node::Element(el) => {
            let e = element_expr(el, flow)?;
            Ok(quote! { ::std::vec![::riode::internal::IntoElement::into_element(#e)] })
        }
        Node::Text(t) => {
            let s = t.value_string();
            Ok(quote! { ::std::vec![::riode::internal::IntoElement::into_element(text::build().child(#s))] })
        }
        Node::Block(b) => {
            let inner = match b {
                rstml::node::NodeBlock::ValidBlock(block) => crate::primitives::attr::strip_braces(block),
                other => quote! { #other },
            };
            Ok(quote! { ::riode::internal::view_child(#inner) })
        }
        Node::Fragment(f) => {
            let kids = children_vec(&f.children, flow)?;
            Ok(quote! { ::riode::internal::view_child(#kids) })
        }
        _ => Err(syn::Error::new_spanned(node, "view!: unsupported child node")),
    }
}

pub fn is_control_child(node: &ViewNode) -> bool {
    let Node::Block(rstml::node::NodeBlock::ValidBlock(inner)) = node else { return false };
    if inner.stmts.len() != 1 { return false; }
    match inner.stmts.first() {
        Some(syn::Stmt::Expr(syn::Expr::Path(p), None)) => {
            p.path.get_ident().is_some_and(|id| crate::engine::flow_util::is_control_placeholder(&id.to_string()))
        }
        _ => false,
    }
}
