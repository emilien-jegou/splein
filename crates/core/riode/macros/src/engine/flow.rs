// Responsibility: Control-flow placeholder registration, warning emits, and AST splicing.

use super::registry::RxEngine;
use super::{attr_node, flow_util, for_node, if_node, match_node};
use proc_macro2::TokenStream as TokenStream2;
use std::cell::Cell;

#[derive(Default)]
pub struct FlowTables {
    pub for_nodes: Vec<for_node::ForNode>,
    pub if_nodes: Vec<if_node::IfNode>,
    pub match_nodes: Vec<match_node::MatchNode>,
    pub attr_raw: Vec<TokenStream2>,
    pub slots: Cell<usize>,
}

impl FlowTables {
    pub fn extract(&mut self, tokens: TokenStream2) -> TokenStream2 {
        let t = for_node::extract(tokens, &mut self.for_nodes);
        let t = if_node::extract(t, &mut self.if_nodes);
        let t = match_node::extract(t, &mut self.match_nodes);
        attr_node::extract(t, &mut self.attr_raw)
    }

    pub fn splice(&self, body: TokenStream2, engine: &RxEngine) -> syn::Result<TokenStream2> {
        flow_util::splice_with(body, &|kind, n, span| self.resolve(kind, n, span, engine))
    }

    pub fn splice_attr_ref(
        &self,
        body: TokenStream2,
        outer: &FlowTables,
        engine: &RxEngine,
    ) -> syn::Result<TokenStream2> {
        flow_util::splice_with(body, &|kind, n, span| match kind {
            "attr" => outer
                .attr_raw
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<attr>` placeholder"))
                .and_then(|raw| attr_node::expand_attr(raw, outer)),
            _ => self.resolve(kind, n, span, engine),
        })
    }

    fn resolve(
        &self,
        kind: &str,
        n: usize,
        span: proc_macro2::Span,
        engine: &RxEngine,
    ) -> syn::Result<TokenStream2> {
        match kind {
            "for" => self
                .for_nodes
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<for>` placeholder"))
                .map(|node| for_node::expand(node, engine)),
            "if" => self
                .if_nodes
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<if>` placeholder"))
                .and_then(|node| if_node::expand(node, engine)),
            "match" => self
                .match_nodes
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<match>` placeholder"))
                .and_then(|node| match_node::expand(node, engine)),
            "attr" => self
                .attr_raw
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<attr>` placeholder"))
                .and_then(|raw| attr_node::expand_attr(raw, self)),
            _ => Err(syn::Error::new(
                span,
                "view!: stale control-flow placeholder",
            )),
        }
    }

    pub fn warn(&self) {
        for n in self
            .for_nodes
            .iter()
            .filter(|n| matches!(n.key, for_node::ForKey::Default))
        {
            // Skip warning for literal ranges:
            if matches!(n.items, syn::Expr::Range(_)) {
                continue;
            }

            proc_macro::Diagnostic::spanned(
            proc_macro2::Span::call_site().unwrap(),
            proc_macro::Level::Warning,
            "<for> without `key=` uses row index as identity. Add `key={.id}`, or silence with `key=_` if index identity is intentional.",
        ).emit();
        }
    }
}
