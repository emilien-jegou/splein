// Single responsibility: the `<for>` control-flow node — token-level pre-pass over
// `view!` input, parsed before rstml sees it.
//
// `<for (i, item) in items key={.id}>…</for>` splices rows directly into the
// parent — control nodes are never layout nodes, so there is deliberately no
// carrier and no container styling on `<for>` itself (style the parent
// `<Group>`; nest one if the design needs it). Without `key=` identity falls
// back to position, with a warning. `key=` names item identity, full stop —
// there is no node to hang a second key on.
// `<for>` ranges are replaced by `{__riode_for_N}` placeholder splices so the
// existing rstml pipeline (`child_expr`, `element_expr`, `<Text>`, components)
// never learns a new grammar. All `<for>` complexity lives in this file.

use crate::flow_util;
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
    Default,
    DotChain(TokenStream2),
    Closure(syn::Expr),
    Path(syn::Path),
}

/// Extracts top-level `<for>…</for>` ranges, replacing each with a `{placeholder}`
/// block. Returns rewritten tokens + side table. Nested `<for>` inside bodies is
/// handled by recursion in `expand_for` (bodies re-enter `super::expand`-adjacent
/// helpers, which call `extract` again).
pub fn extract(tokens: TokenStream2, out_nodes: &mut Vec<ForNode>) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<proc_macro2::TokenTree> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if flow_util::is_open(&toks, i, "for") {
            let start = i;
            // Provisional header end (angle-counting, operator-aware); the
            // enclosing close needs no exact header, so find it first and
            // backtrack the longest fully-parsing header (lone-`>` proof).
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
                    // Still consume the range so one bad `<for>` doesn't cascade.
                }
            }
            i = close_end + 1;
        } else {
            // Only top-level scan here; bodies re-enter via expand recursion.
            // `{...}` brace groups at top level can't contain RSX directly, so skip.
            out.push(toks[i].clone());
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// Parses header tokens `<for PAT in ITEMS [key={KEY}]>`.
///
/// `key=` is the only clause: it names item identity (`.id` shorthand,
/// closure, or path). Anything else trailing is rejected — `<for>` contributes
/// rows, not a node, so direction, gap, `ref`, and keys belong on the parent
/// `<Group>` (nest one if the design needs it). `PAT` is `x` or `(i, x)`.
fn parse_header(
    header: &[proc_macro2::TokenTree],
) -> syn::Result<(Option<syn::Ident>, syn::Ident, syn::Expr, ForKey)> {
    use proc_macro2::TokenTree::*;
    // Strip leading `<` `for` and trailing `>`.
    let inner: TokenStream2 = header[2..header.len() - 1].iter().cloned().collect();
    let inner_toks: Vec<proc_macro2::TokenTree> = inner.into_iter().collect();
    // Split depth-0 `in`.
    let in_pos = flow_util::find_ident_depth0(&inner_toks, "in").ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<for>` needs `in` — `<for item in items key={.id}>`",
        )
    })?;
    let pat_ts: TokenStream2 = inner_toks[..in_pos].iter().cloned().collect();
    let (index, item) = parse_pattern(pat_ts)?;
    // The items expression runs to the first depth-0 `name =` clause start
    // (single `=`; `==`/`=>` inside the expression don't qualify).
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
    // Optional trailing `key={…}` — and nothing else.
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
                "view!: `<for>` takes `key={…}` — `key={.id}`, `key={|s| …}`, or `key={f}`",
            )
        })?;
        match val {
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
                    "view!: `<for>` takes `key={…}` — `key={.id}`, `key={|s| …}`, or `key={f}`",
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

/// `toks[j]` is a lone `=` (assignment position), not `==` / `=>` / `>=`.
fn is_single_eq(toks: &[proc_macro2::TokenTree], j: usize) -> bool {
    use proc_macro2::TokenTree::*;
    if !matches!(toks.get(j), Some(Punct(p)) if p.as_char() == '=') {
        return false;
    }
    !matches!(toks.get(j + 1), Some(Punct(p)) if p.as_char() == '=' || p.as_char() == '>')
}

fn parse_pattern(pat_ts: TokenStream2) -> syn::Result<(Option<syn::Ident>, syn::Ident)> {
    use syn::parse::Parser;
    // `(i, x)` tuple or bare `x`.
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
    if matches!(itoks.first(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '.') {
        // `.chain` → prefix later with generated param.
        return Ok(ForKey::DotChain(inner));
    }
    // Closure (`|…|` / `move`) or path — try closure, then path, else generic expr.
    if let Ok(c) = syn::parse2::<syn::ExprClosure>(inner.clone()) {
        return Ok(ForKey::Closure(syn::Expr::Closure(c)));
    }
    if let Ok(p) = syn::parse2::<syn::Path>(inner.clone()) {
        return Ok(ForKey::Path(p));
    }
    let e: syn::Expr = syn::parse2(inner)?;
    Ok(ForKey::Closure(e))
}

/// Emits a `ChildSegment` for one `<for>` node: rows splice directly into the
/// parent, with no carrier node between.
///
/// Row bodies are segment vecs, so nesting composes: a single-root row keeps
/// its key (which is the entire point of keyed lists), multi-root rows append
/// in order with each key intact, and nested control nodes stay dynamic.
pub fn expand(node: &ForNode) -> TokenStream2 {
    let row_vec = match crate::view::expand_children_vec(node.body.clone()) {
        Ok(v) => v,
        Err(err) => return err.to_compile_error(),
    };
    let items = &node.items;
    let item = &node.item;
    // Row closure params: user index name or `_i` when omitted (no unused warning:
    // explicit `_` prefix is warning-quiet by construction).
    let idx_pat = node
        .index
        .clone()
        .map(|i| quote! { #i })
        .unwrap_or_else(|| quote! { _i });
    // Unused `item` binding is the user's own spelling; keep warning (agreed).
    let row = quote! { move |#idx_pat: usize, #item| -> ::std::vec::Vec<::riode::dsl::ChildSegment> { #row_vec } };
    match &node.key {
        ForKey::Default => {
            quote! {
                {
                    ::riode::dsl::ChildSegment::keyed_index(#items, #row)
                }
            }
        }
        ForKey::DotChain(chain) => {
            quote! {
                {
                    ::riode::dsl::ChildSegment::keyed(#items, |__riode_it| __riode_it #chain, #row)
                }
            }
        }
        ForKey::Closure(c) => {
            quote! { { ::riode::dsl::ChildSegment::keyed(#items, #c, #row) } }
        }
        ForKey::Path(p) => {
            quote! { { ::riode::dsl::ChildSegment::keyed(#items, #p, #row) } }
        }
    }
}
