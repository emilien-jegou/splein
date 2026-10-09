// Single responsibility: the `<match>` control-flow node — token-level pre-pass.
//
// `<match scrut>` takes only `<case PAT [if GUARD]>…</case>` children and
// desugars to `group().children_of(move || match scrut { PAT => vec![…], … })`.
// Arm patterns are real Rust patterns (bindings scope the arm body free;
// exhaustiveness is checked by rustc on the generated code). Signals read in
// the scrutinee, guards, or arm bodies resubscribe, so arms are reactive.

use crate::flow_util;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::Parser;

/// One extracted `<match>` node: scrutinee + raw RSX bodies per arm.
pub struct MatchArm {
    pub pat: syn::Pat,
    pub guard: Option<syn::Expr>,
    pub body: TokenStream2,
}

/// One extracted `<match>` node.
pub struct MatchNode {
    pub scrut: syn::Expr,
    pub arms: Vec<MatchArm>,
}

/// Extracts top-level `<match>…</match>` ranges, replacing each with a
/// placeholder. Mirrors [`crate::for_node::extract`].
pub fn extract(tokens: TokenStream2, out_nodes: &mut Vec<MatchNode>) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<proc_macro2::TokenTree> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if flow_util::is_open(&toks, i, "match") {
            let start = i;
            let prov = flow_util::find_header_end(&toks, start);
            let from = prov.map(|h| h + 1).unwrap_or(start + 2);
            let Some((body_end, close_end)) = flow_util::find_close(&toks, from, "match")
            else {
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
            match parse(&header, &toks, header_end + 1, body_end, body) {
                Ok(node) => {
                    let n = out_nodes.len();
                    out_nodes.push(node);
                    out.push(flow_util::placeholder("match", n));
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

fn parse(
    header: &[proc_macro2::TokenTree],
    toks: &[proc_macro2::TokenTree],
    arm_start: usize,
    match_close_lt: usize,
    body: TokenStream2,
) -> syn::Result<MatchNode> {
    let scrut = parse_header_shape(header)?;
    let arms = split_arms(toks, arm_start, match_close_lt, body)?;
    if arms.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<match>` needs at least one `<case PAT>` arm",
        ));
    }
    Ok(MatchNode { scrut, arms })
}

/// Validates/shapes a `<match SCRUT>` header. Split from [`parse`] so
/// backtracking can probe candidates without bodies.
fn parse_header_shape(header: &[proc_macro2::TokenTree]) -> syn::Result<syn::Expr> {
    // Header is `<match SCRUT>`: everything between `match` and `>` is a Rust expr.
    let scrut_ts: TokenStream2 = header[2..header.len() - 1].iter().cloned().collect();
    if scrut_ts.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<match>` needs a scrutinee — `<match status.get()>`",
        ));
    }
    syn::parse2(scrut_ts)
}

/// Splits a `<match>` body into `<case>` arms. Anything that is not a `<case>`
/// range is an error — plain RSX cannot sit directly inside `<match>`.
/// Nested `<match>` ranges are opaque via the `case`-depth tally in
/// [`flow_util::find_close`]: an inner arm bumps the count and its close
/// drops it, so outer slicing stays correct.
///
/// Walks the original token coordinates (`arm_start..match_close_lt`) so arm
/// headers backtrack against the enclosing close — guards with lone `>`
/// (`<case Ok(x) if x > 0>`) resolve like every other header.
fn split_arms(
    toks: &[proc_macro2::TokenTree],
    arm_start: usize,
    match_close_lt: usize,
    _body: TokenStream2,
) -> syn::Result<Vec<MatchArm>> {
    let mut arms = Vec::new();
    let mut j = arm_start;
    while j < match_close_lt {
        if !flow_util::is_open(toks, j, "case") {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<match>` takes only `<case PAT>` children — wrap anything else in an arm",
            ));
        }
        let prov = flow_util::find_header_end(toks, j);
        let from = prov.map(|h| h + 1).unwrap_or(j + 2);
        let Some((arm_body_lt, close_gt)) = flow_util::find_close(toks, from, "case") else {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<case …>` needs a matching `</case>`",
            ));
        };
        let header_end = flow_util::refine_header_end(toks, j, arm_body_lt, prov, |slice| {
            parse_arm_shape(slice).is_ok()
        });
        let Some(header_end) = header_end else {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "view!: `<case …>` needs a closing `>`",
            ));
        };
        let header: Vec<_> = toks[j..=header_end].to_vec();
        let arm_body: TokenStream2 = toks[header_end + 1..arm_body_lt].iter().cloned().collect();
        arms.push(parse_arm(&header, arm_body)?);
        // Skip past `</case …>`.
        j = close_gt + 1;
    }
    Ok(arms)
}

/// Header is `<case PAT [if GUARD]>`. `PAT` may be `|`-separated
/// (`<case A | B>`); `if` splits at depth 0 (`if` cannot bind in patterns,
/// so the split is unambiguous).
fn parse_arm(
    header: &[proc_macro2::TokenTree],
    body: TokenStream2,
) -> syn::Result<MatchArm> {
    let (pat, guard) = parse_arm_shape(header)?;
    Ok(MatchArm { pat, guard, body })
}

/// Validates/shapes a `<case>` header without its body, so backtracking can
/// probe candidates. Split from [`parse_arm`].
fn parse_arm_shape(header: &[proc_macro2::TokenTree]) -> syn::Result<(syn::Pat, Option<syn::Expr>)> {
    let inner: TokenStream2 = header[2..header.len() - 1].iter().cloned().collect();
    let inner_toks: Vec<proc_macro2::TokenTree> = inner.clone().into_iter().collect();
    let (pat_ts, guard) = match flow_util::find_ident_depth0(&inner_toks, "if") {
        Some(k) => {
            let pat_ts: TokenStream2 = inner_toks[..k].iter().cloned().collect();
            let guard_ts: TokenStream2 = inner_toks[k + 1..].iter().cloned().collect();
            if guard_ts.is_empty() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "view!: `<case PAT if …>` needs a guard expression",
                ));
            }
            (pat_ts, Some(syn::parse2::<syn::Expr>(guard_ts)?))
        }
        None => (inner, None),
    };
    if pat_ts.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: `<case>` needs a pattern — `<case Status::Loading>` or `<case _>`",
        ));
    }
    let pat: syn::Pat = syn::Pat::parse_multi_with_leading_vert.parse2(pat_ts)?;
    Ok((pat, guard))
}

/// Emits a `ChildSegment::Dynamic` for one node: the active arm's segments
/// splice directly into the parent, with no carrier node between.
/// Arm bodies expand via [`crate::view::expand_children_vec`].
pub fn expand(node: &MatchNode) -> syn::Result<TokenStream2> {
    let scrut = &node.scrut;
    let mut arms = Vec::new();
    for arm in &node.arms {
        let pat = &arm.pat;
        let body = crate::view::expand_children_vec(arm.body.clone())?;
        if let Some(g) = &arm.guard {
            arms.push(quote! { #pat if #g => #body, });
        } else {
            arms.push(quote! { #pat => #body, });
        }
    }
    Ok(quote! {
        ::aurora::dsl::ChildSegment::Dynamic(::std::rc::Rc::new(move || match #scrut { #(#arms)* }))
    })
}
