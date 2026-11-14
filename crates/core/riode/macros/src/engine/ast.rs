// Responsibility: Concrete RSTML AST node types, token inspection, and element expression lowering.

use super::children::children_vec;
use super::flow::FlowTables;
use super::registry::{EngineContext, RxEngine};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{Infallible, Node, NodeAttribute, NodeElement, NodeName};

pub type Element = NodeElement<Infallible>;
pub type ViewNode = Node<Infallible>;

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

pub fn element_expr(
    el: &Element,
    flow: &FlowTables,
    engine: &RxEngine,
) -> syn::Result<TokenStream2> {
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
        let kids = children_vec(&el.children, flow, engine)?;
        chain = quote! { #chain.children(#kids) };
    }
    Ok(quote! { #chain.build() })
}
