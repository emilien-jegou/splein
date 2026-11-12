// Single responsibility: Concrete RSTML AST node types, token inspection, and template shape serialization.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{Infallible, Node, NodeAttribute, NodeElement, NodeName};
use super::children::children_vec;
use super::flow::FlowTables;
use super::registry::{EngineContext, RxEngine};

pub type Element = NodeElement<Infallible>;
pub type ViewNode = Node<Infallible>;

pub fn node_name_string(name: &NodeName) -> String {
    match name {
        NodeName::Path(p) => p.path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::"),
        other => other.to_string().trim().to_string(),
    }
}

pub fn element_expr(el: &Element, flow: &FlowTables, engine: &RxEngine) -> syn::Result<TokenStream2> {
    let tag = node_name_string(el.name());

    if let Some(prim) = engine.get(&tag) {
        return (prim.expand)(EngineContext { el, flow, engine });
    }

    if tag.chars().next().is_some_and(|c| c.is_lowercase()) {
        return Err(syn::Error::new_spanned(
            el.name(),
            format!("view!: unknown tag `<{tag}>`. Custom components must be PascalCase."),
        ));
    }

    // Generic PascalCase component invocation builder
    let props = format_ident!("{}Props", tag);
    let mut chain = quote! { #props::builder() };
    for a in el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if key == "children" {
                return Err(syn::Error::new_spanned(&attr.key, "view!: pass children as nested RSX, not `children=`"));
            }
            let method = format_ident!("{}", key.replace(['-', ':'], "_"));
            let val = crate::primitives::attr::attr_value(&key, attr, flow);
            chain = quote! { #chain.#method(#val) };
        }
    }
    if !el.children.is_empty() {
        let kids = children_vec(&el.children, flow, engine)?;
        chain = quote! { #chain.children(#kids) };
    }
    Ok(quote! { #chain.build() })
}

pub fn shape_of_nodes(nodes: &[ViewNode]) -> String {
    let mut out = String::from("[");
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push_str(&shape_of_node(n));
    }
    out.push(']');
    out
}

fn shape_of_node(node: &ViewNode) -> String {
    match node {
        Node::Element(el) => {
            let tag = node_name_string(el.name());
            let mut attrs: Vec<String> = el.attributes().iter().filter_map(|a| match a {
                NodeAttribute::Attribute(attr) => Some(node_name_string(&attr.key)),
                _ => None,
            }).collect();
            attrs.sort();
            format!("{tag}{{{}}}({})", attrs.join(","), shape_of_nodes(&el.children))
        }
        Node::Text(_) => "text".to_string(),
        Node::Block(_) => "{expr}".to_string(),
        Node::Fragment(f) => format!("frag{}", shape_of_nodes(&f.children)),
        _ => "unknown".to_string(),
    }
}
