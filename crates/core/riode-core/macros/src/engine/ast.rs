// Responsibility: Concrete RSTML AST node types, token inspection, and element expression lowering.

use super::children::children_vec;
use super::flow::FlowTables;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{Infallible, Node, NodeAttribute, NodeElement, NodeName};

pub type Element = NodeElement<Infallible>;
pub type ViewNode = Node<Infallible>;

/// Per-element expansion inputs: the element and the enclosing control-flow tables.
pub struct EngineContext<'a> {
    pub el: &'a Element,
    pub flow: &'a FlowTables,
}

pub fn node_name_string(name: &NodeName) -> String {
    match name {
        NodeName::Path(p) => p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        other => other.to_string().trim().to_string(),
    }
}

pub fn element_expr(el: &Element, flow: &FlowTables) -> syn::Result<TokenStream2> {
    let tag = node_name_string(el.name());
    let cx = EngineContext { el, flow };

    match tag.as_str() {
        // `g` is the only privileged tag: flow splicing needs child segments.
        // Built-ins are lowercase only — PascalCase is the component namespace.
        "g" => return crate::primitives::group::expand(cx),
        _ => {}
    }

    if tag.chars().next().is_some_and(|c| c.is_lowercase()) {
        return super::generic::expand(cx);
    }

    let props = format_ident!("{}Props", tag);
    let mut chain = quote! { #props::builder() };
    for a in el.attributes() {
        if let NodeAttribute::Attribute(attr) = a {
            let key = node_name_string(&attr.key);
            if key == "children" {
                return Err(syn::Error::new_spanned(
                    &attr.key,
                    "view!: pass children as nested RSX, not `children=`",
                ));
            }
            let method = format_ident!("{}", key.replace(['-', ':'], "_"));
            let val = crate::primitives::attr::attr_value(&key, attr);
            chain = quote! { #chain.#method(#val) };
        }
    }
    if !el.children.is_empty() {
        let kids = children_vec(&el.children, flow)?;
        chain = quote! { #chain.children(#kids) };
    }
    Ok(quote! { #chain.build() })
}
