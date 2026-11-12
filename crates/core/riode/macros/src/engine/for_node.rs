// Responsibility: The `<for>` control-flow node — token-level pre-pass over `view!` input.

use crate::engine::children::expand_children_vec;
use crate::engine::flow_util;
use crate::engine::registry::RxEngine;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// One extracted `<for>` node: parsed header + raw body tokens for recursive expansion.
pub struct ForNode {
    pub index: Option<syn::Ident>,
    pub item: syn::Ident,
    pub items: syn::Expr,
    pub key: ForKey,
    pub body: TokenStream2,
}

/// `key=` forms: omitted (index default), `.chain` shorthand, closure, or fn path.
pub enum ForKey {
    /// Omitted key= (warns if not a range)
    Default,
    /// Explicit positional index: `key=_` or `key={_}` (never warns)
    Index,
    DotChain(TokenStream2),
    Closure(syn::Expr),
    Path(syn::Path),
}

/// Extracts top-level `<for>…</for>` ranges, replacing each with a `{placeholder}` block.
pub fn extract(tokens: TokenStream2, out_nodes: &mut Vec<ForNode>) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<proc_macro2::TokenTree> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if flow_util::is_open(&toks, i, "for") {
            let start = i;
            let prov = flow_util::find_header_end(&toks, start);
            let from = prov.map(|h| h + 1).unwrap_or(start + 2);
            let Some((body_end, close_end)) = flow_util::find_close(&toks, from, "for") else {
                out.extend(toks[start..].iter().cloned());
                break;
            };
            let header_end = flow_util::refine_header_end(&toks, start, body_end, prov, |slice| {
                parse_header(slice).is_ok()
            });
            let Some(header_end) = header_end else {
                out.extend(toks[start..].iter().cloned());
                break;
            };
            let header: Vec<_> = toks[start..=header_end].to_vec();
            let body: TokenStream2 = toks[header_end + 1..body_end].iter().cloned().collect();
            match parse_header(&header) {
                Ok((index, item, items, key)) => {
                    let n = out_nodes.len();
                    out_nodes.push(ForNode {
                        index,
                        item,
                        items,
                        key,
                        body,
                    });
                    out.push(flow_util::placeholder("for", n));
                }
                Err(e) => {
                    let err = e.to_compile_error();
                    out.extend(err.into_iter());
                }
            }
            i = close_end + 1;
        } else {
            out.push(toks[i].clone());
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// Parses header tokens `<for PAT in ITEMS [key={KEY}]>`.
fn parse_header(
    header: &[proc_macro2::TokenTree],
) -> syn::Result<(Option<syn::Ident>, syn::Ident, syn::Expr, ForKey)> {
    use proc_macro2::TokenTree::*;
    let inner: TokenStream2 = header[2..header.len() - 1].iter().cloned().collect();
    let inner_toks: Vec<proc_macro2::TokenTree> = inner.into_iter().collect();
    let in_pos = flow_util::find_ident_depth0(&inner_toks, "in").ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<for>` needs `in` — `<for item in items key={.id}>`",
        )
    })?;
    let pat_ts: TokenStream2 = inner_toks[..in_pos].iter().cloned().collect();
    let (index, item) = parse_pattern(pat_ts)?;
    let rest: Vec<_> = inner_toks[in_pos + 1..].to_vec();
    let mut split_at = rest.len();
    let mut j = 0;
    while j + 1 < rest.len() {
        if matches!(&rest[j], Ident(_)) && is_single_eq(&rest, j + 1) {
            split_at = j;
            break;
        }
        j += 1;
    }
    let items_ts: TokenStream2 = rest[..split_at].iter().cloned().collect();
    if items_ts.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<for>` needs an items expression — `<for x in items>`",
        ));
    }
    let items: syn::Expr = syn::parse2(items_ts)?;
    let mut key = ForKey::Default;
    if split_at < rest.len() {
        let name = match &rest[split_at] {
            Ident(id) => id.to_string(),
            _ => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<for>` takes only `key={…}` after the items — `<for x in xs key={.id}>`",
                ));
            }
        };
        if name != "key" || !is_single_eq(&rest, split_at + 1) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "view!: `<for>` has no `{name}=` — it contributes rows, not a node, so style the parent `<Group>` instead"
                ),
            ));
        }
        let val = rest.get(split_at + 2).ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<for>` takes `key={…}` — `key={.id}`, `key={|s| …}`, or `key=_`",
            )
        })?;
        match val {
            Ident(id) if id == "_" => {
                key = ForKey::Index;
            }
            Literal(_) => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `key=` on `<for>` names item identity (a function of each item) — to key the list itself, put `key=` on the parent `<Group>`",
                ));
            }
            Group(g) if g.delimiter() == proc_macro2::Delimiter::Brace => {
                key = parse_key(g.stream())?;
            }
            _ => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<for>` takes `key={…}` — `key={.id}`, `key={|s| …}`, or `key=_`",
                ));
            }
        }
        if split_at + 3 < rest.len() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<for>` takes only `key={…}` after the items — style the parent `<Group>` instead",
            ));
        }
    }
    Ok((index, item, items, key))
}

fn is_single_eq(toks: &[proc_macro2::TokenTree], j: usize) -> bool {
    use proc_macro2::TokenTree::*;
    if !matches!(toks.get(j), Some(Punct(p)) if p.as_char() == '=') {
        return false;
    }
    !matches!(toks.get(j + 1), Some(Punct(p)) if p.as_char() == '=' || p.as_char() == '>')
}

fn parse_pattern(pat_ts: TokenStream2) -> syn::Result<(Option<syn::Ident>, syn::Ident)> {
    use syn::parse::Parser;
    if let Ok(syn::Pat::Tuple(pair)) = syn::Pat::parse_single.parse2(pat_ts.clone()) {
        if pair.elems.len() != 2 {
            return Err(syn::Error::new_spanned(
                &pat_ts,
                "view!: `<for>` pattern is `x` or `(i, x)`",
            ));
        }
        let mut it = pair.elems.iter();
        let a = it.next().unwrap();
        let b = it.next().unwrap();
        let ai = pat_ident(a)?;
        let bi = pat_ident(b)?;
        return Ok((Some(ai), bi));
    }
    let id: syn::Ident = syn::parse2(pat_ts)?;
    Ok((None, id))
}

fn pat_ident(pat: &syn::Pat) -> syn::Result<syn::Ident> {
    match pat {
        syn::Pat::Ident(pi) => Ok(pi.ident.clone()),
        other => Err(syn::Error::new_spanned(
            other,
            "view!: `<for>` pattern binds plain idents only",
        )),
    }
}

fn parse_key(val_ts: TokenStream2) -> syn::Result<ForKey> {
    let inner = flow_util::unbrace(&val_ts);
    let itoks: Vec<proc_macro2::TokenTree> = inner.clone().into_iter().collect();

    // Explicit index opt-in: `key={_}`
    if itoks.len() == 1 {
        if let Some(proc_macro2::TokenTree::Ident(id)) = itoks.first() {
            if id == "_" {
                return Ok(ForKey::Index);
            }
        }
    }

    if matches!(itoks.first(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '.') {
        return Ok(ForKey::DotChain(inner));
    }
    if let Ok(c) = syn::parse2::<syn::ExprClosure>(inner.clone()) {
        return Ok(ForKey::Closure(syn::Expr::Closure(c)));
    }
    if let Ok(p) = syn::parse2::<syn::Path>(inner.clone()) {
        return Ok(ForKey::Path(p));
    }
    let e: syn::Expr = syn::parse2(inner)?;
    Ok(ForKey::Closure(e))
}

pub fn expand(node: &ForNode, engine: &RxEngine) -> TokenStream2 {
    let row_vec = match expand_children_vec(node.body.clone(), engine) {
        Ok(v) => v,
        Err(err) => return err.to_compile_error(),
    };
    let items = &node.items;
    let item = &node.item;
    let idx_pat = node
        .index
        .clone()
        .map(|i| quote! { #i })
        .unwrap_or_else(|| quote! { _i });
    let row = quote! { move |#idx_pat: usize, #item| -> ::std::vec::Vec<::riode::internal::ChildSegment> { #row_vec } };
    match &node.key {
        ForKey::Default | ForKey::Index => {
            quote! {
                {
                    ::riode::internal::ChildSegment::keyed_index(#items, #row)
                }
            }
        }
        ForKey::DotChain(chain) => {
            quote! {
                {
                    ::riode::internal::ChildSegment::keyed(#items, |__riode_it| __riode_it #chain, #row)
                }
            }
        }
        ForKey::Closure(c) => {
            quote! { { ::riode::internal::ChildSegment::keyed(#items, #c, #row) } }
        }
        ForKey::Path(p) => {
            quote! { { ::riode::internal::ChildSegment::keyed(#items, #p, #row) } }
        }
    }
}
