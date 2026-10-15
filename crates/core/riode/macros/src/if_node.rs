// Single responsibility: the `<if>` control-flow node — token-level pre-pass.
//
// `<if cond>…</if>` (+ `<else if c>` / `<else>` segments) desugars to
// `group().children_of(move || if cond { vec![…] } …)`. Signals read inside
// the builder resubscribe on change, so branches are reactive with no engine
// work. A missing `<else>` means an empty branch. Branch swaps remount —
// hooks inside a branch do not survive toggling.

use crate::flow_util;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// One extracted `<if>` node: condition + raw RSX bodies per segment.
pub struct IfNode {
    pub cond: syn::Expr,
    pub then_body: TokenStream2,
    pub elifs: Vec<(syn::Expr, TokenStream2)>,
    pub else_body: Option<TokenStream2>,
}

/// Extracts top-level `<if>…</if>` ranges, replacing each with a placeholder.
/// Mirrors [`crate::for_node::extract`]; bodies re-enter expansion on splice.
pub fn extract(tokens: TokenStream2, out_nodes: &mut Vec<IfNode>) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<proc_macro2::TokenTree> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if flow_util::is_open(&toks, i, "if") {
            let start = i;
            let prov = flow_util::find_header_end(&toks, start);
            let from = prov.map(|h| h + 1).unwrap_or(start + 2);
            let Some((body_end, close_end)) = flow_util::find_close(&toks, from, "if") else {
                out.extend(toks[start..].iter().cloned());
                break;
            };
            let header_end = flow_util::refine_header_end(&toks, start, body_end, prov, |slice| {
                parse_header_shape(slice).is_ok()
            });
            let Some(header_end) = header_end else {
                out.extend(toks[start..].iter().cloned());
                break;
            };
            let header: Vec<_> = toks[start..=header_end].to_vec();
            let body: TokenStream2 = toks[header_end + 1..body_end].iter().cloned().collect();
            match parse(&header, body) {
                Ok(node) => {
                    let n = out_nodes.len();
                    out_nodes.push(node);
                    out.push(flow_util::placeholder("if", n));
                }
                Err(e) => {
                    out.extend(e.to_compile_error().into_iter());
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

fn parse(header: &[proc_macro2::TokenTree], body: TokenStream2) -> syn::Result<IfNode> {
    let (cond, (then_body, elifs, else_body)) =
        (parse_header_shape(header)?, split_branches(body)?);
    Ok(IfNode {
        cond,
        then_body,
        elifs,
        else_body,
    })
}

/// Validates/shapes an `<if COND>` header: everything between `if` and `>` is
/// one Rust expression. Split from [`parse`] so backtracking can probe
/// candidates without bodies.
fn parse_header_shape(header: &[proc_macro2::TokenTree]) -> syn::Result<syn::Expr> {
    // Header is `<if COND>`: everything between `if` and `>` is a Rust expr.
    let cond_ts: TokenStream2 = header[2..header.len() - 1].iter().cloned().collect();
    if cond_ts.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<if>` needs a condition — `<if score.get() >= 90>`",
        ));
    }
    syn::parse2(cond_ts)
}

/// Splits an `<if>` body into then + `<else …>…</else>` ranges.
/// Nested `<if>` ranges are opaque (depth-tallied, like [`flow_util::find_close`]).
///
/// Shape: `THEN (<else>SEG</else> | <else if C>SEG</else>)*` — ranges tile the
/// body exactly; content stranded between `</else>` and the next `<else>` (or
/// `</if>`) is an error, as are `<else if>` after `<else>` and double `<else>`.
fn split_branches(
    body: TokenStream2,
) -> syn::Result<(
    TokenStream2,
    Vec<(syn::Expr, TokenStream2)>,
    Option<TokenStream2>,
)> {
    let toks: Vec<proc_macro2::TokenTree> = body.into_iter().collect();
    // Phase 1: depth-0 `<else…>` *open* positions (closes excluded by the
    // `!/'` guard; nested `<if>` ranges skipped by tally).
    let mut opens: Vec<usize> = Vec::new();
    let mut depth = 0i32;
    let mut j = 0;
    while j < toks.len() {
        if flow_util::is_open(&toks, j, "if") {
            depth += 1;
            j += 1;
            continue;
        }
        if flow_util::is_close(&toks, j, "if") {
            depth -= 1;
            j += 1;
            continue;
        }
        if depth == 0
            && matches!(toks.get(j), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '<')
            && matches!(toks.get(j + 1), Some(proc_macro2::TokenTree::Ident(id)) if id == "else")
            && !matches!(toks.get(j + 2), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '/')
        {
            opens.push(j);
        }
        j += 1;
    }
    let then_body: TokenStream2 = toks[..opens.first().copied().unwrap_or(toks.len())]
        .iter()
        .cloned()
        .collect();
    let mut elifs = Vec::new();
    let mut else_body = None;
    let mut seen_else = false;
    for (idx, open) in opens.iter().enumerate() {
        let open = *open;
        // Open shape: `<else>` or `<else if COND>`.
        let kind = if matches!(toks.get(open + 2), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '>')
        {
            1u8
        } else if matches!(toks.get(open + 2), Some(proc_macro2::TokenTree::Ident(id)) if id == "if")
        {
            0u8
        } else {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<if>` children are `<else if …>…</else>` or `<else>…</else>`, not `<else …>`",
            ));
        };
        // Segment bound: next else-open, else end of body.
        let bound = opens.get(idx + 1).copied().unwrap_or(toks.len());
        // Segment starts after the open tag: `<else>` is 3 tokens; elif
        // conditions backtrack to the longest fully-parsing expr.
        let seg_start = if kind == 1 {
            open + 3
        } else {
            elif_cond_end(&toks, open + 3, bound)
                .map(|gt| gt + 1)
                .ok_or_else(|| {
                    syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "view!: `<else if …>` needs a condition and closing `>` — `<else if score.get() >= 60>`",
                    )
                })?
        };
        // Balanced `</else>` from the segment start (nested `<if>` elses tally).
        let Some((seg_lt, close_gt)) = flow_util::find_close(&toks, seg_start, "else") else {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<else…>` needs a matching `</else>`",
            ));
        };
        if seg_lt >= bound {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<else…>` needs its `</else>` before the next `<else>`",
            ));
        }
        // Close tag is bare `</else>`: `<`, `/`, `else`, `>` exactly.
        if !matches!(toks.get(close_gt.saturating_sub(1)), Some(proc_macro2::TokenTree::Ident(id)) if id == "else")
        {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `</else>` takes no attributes",
            ));
        }
        let seg: TokenStream2 = toks[seg_start..seg_lt].iter().cloned().collect();
        if kind == 1 {
            if seen_else {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<if>` takes at most one `<else>`, last",
                ));
            }
            seen_else = true;
            else_body = Some(seg);
        } else {
            if seen_else {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<else if …>` after `<else>` — move it before",
                ));
            }
            let cond_ts: TokenStream2 = toks[open + 3..seg_start - 1].iter().cloned().collect();
            if cond_ts.is_empty() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<else if>` needs a condition — `<else if score.get() >= 60>`",
                ));
            }
            let cond: syn::Expr = syn::parse2(cond_ts)?;
            elifs.push((cond, seg));
        }
        // Ranges tile: `</else>` must reach the next `<else>` (or body end).
        if close_gt + 1 != bound {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: content between `</else>` and the next `<else>` is not allowed — move it inside a branch",
            ));
        }
    }
    Ok((then_body, elifs, else_body))
}

/// End (index of closing `>`) of an `<else if COND>`: the last bare-`>`
/// before `bound` whose `cond_start..=cand` parses as one expression.
/// Skips `/>` and `=>`; falls back to the first depth-0 `>` (legacy).
fn elif_cond_end(
    toks: &[proc_macro2::TokenTree],
    cond_start: usize,
    bound: usize,
) -> Option<usize> {
    use proc_macro2::TokenTree::*;
    let mut cands: Vec<usize> = Vec::new();
    let mut depth = 0i32;
    let mut j = cond_start;
    while j < bound.min(toks.len()) {
        match &toks[j] {
            Group(_) => {}
            Punct(p) if p.as_char() == '>' && is_op_cont(toks, j) => {}
            Punct(p) if matches!(p.as_char(), '<' | '(' | '[' | '{') => depth += 1,
            Punct(p) if p.as_char() == '>' && depth == 0 => cands.push(j),
            Punct(p) if matches!(p.as_char(), '>' | ')' | ']' | '}') => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    for cand in cands.into_iter().rev() {
        let ts: TokenStream2 = toks[cond_start..cand].iter().cloned().collect();
        if syn::parse2::<syn::Expr>(ts).is_ok() {
            return Some(cand);
        }
    }
    None
}

fn is_op_cont(toks: &[proc_macro2::TokenTree], j: usize) -> bool {
    matches!(
        toks.get(j + 1),
        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '=' || p.as_char() == '>'
    )
}

/// Emits a `ChildSegment::Dynamic` for one node: the active arm's segments
/// splice directly into the parent, with no carrier node between.
/// Arm bodies expand via [`crate::view::expand_children_vec`] (recursion
/// handles nested control flow). No `<else>` means an empty fallback branch.
pub fn expand(node: &IfNode) -> syn::Result<TokenStream2> {
    let cond = &node.cond;
    let then_v = crate::view::expand_children_vec(node.then_body.clone())?;
    let mut out = quote! { if #cond { #then_v } };
    for (c, b) in &node.elifs {
        let v = crate::view::expand_children_vec(b.clone())?;
        out = quote! { #out else if #c { #v } };
    }
    if let Some(e) = &node.else_body {
        let v = crate::view::expand_children_vec(e.clone())?;
        out = quote! { #out else { #v } };
    } else {
        out = quote! { #out else { ::std::vec![] } };
    }
    Ok(quote! {
        ::riode::internal::ChildSegment::Dynamic(::std::rc::Rc::new(move || #out))
    })
}
