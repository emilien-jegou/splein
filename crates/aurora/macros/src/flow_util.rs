// Single responsibility: shared token scanners for the `view!` control-flow
// pre-pass (`<for>`, `<if>`, `<match>`).
//
// Each control node is extracted at the raw `proc_macro2` level before rstml
// parses, replaced by a `{__aurora_<kind>_N}` placeholder splice, and expanded
// back after codegen via [`splice`]. The scanners below are grammar-free:
// tag matching, header-end search, nested close-tag search, depth-0 ident
// search, and brace-strip. Node-specific header parsing lives in each node's
// own module.

use proc_macro2::TokenStream as TokenStream2;

/// `<` `tag` not followed by `/`.
pub fn is_open(toks: &[proc_macro2::TokenTree], i: usize, tag: &str) -> bool {
    use proc_macro2::TokenTree::*;
    matches!(
        toks.get(i),
        Some(Punct(p)) if p.as_char() == '<'
    ) && matches!(
        toks.get(i + 1),
        Some(Ident(id)) if id == tag
    ) && !matches!(
        toks.get(i + 2),
        Some(Punct(p)) if p.as_char() == '/'
    )
}

/// `<` `/` `tag`.
pub fn is_close(toks: &[proc_macro2::TokenTree], i: usize, tag: &str) -> bool {
    use proc_macro2::TokenTree::*;
    matches!(
        toks.get(i),
        Some(Punct(p)) if p.as_char() == '<'
    ) && matches!(
        toks.get(i + 1),
        Some(Punct(p)) if p.as_char() == '/'
    ) && matches!(
        toks.get(i + 2),
        Some(Ident(id)) if id == tag
    )
}

/// Index of the `>` closing the `<tag …>` header starting at `start`.
/// `Group` tokens are opaque (their insides can't contain a bare header `<`/`>`).
///
/// Operator-aware: `>`/`>>`/`>=` and `<`/`<<`/`<=` pairs are skipped, so
/// `>=`/`<=`/shifts in header expressions don't terminate the scan early.
/// A *lone* `<`/`>` comparison (`a < b`) is still ambiguous with tags — use
/// [`refine_header_end`] after the enclosing close is known, or parenthesize.
pub fn find_header_end(toks: &[proc_macro2::TokenTree], start: usize) -> Option<usize> {
    use proc_macro2::TokenTree::*;
    let mut depth_angle = 0i32;
    let mut j = start;
    while j < toks.len() {
        match &toks[j] {
            Punct(p) if p.as_char() == '<' && next_is(&toks, j, &['=', '<']) => j += 1,
            Punct(p) if p.as_char() == '>' && next_is(&toks, j, &['=', '>']) => j += 1,
            Punct(p) if p.as_char() == '<' => depth_angle += 1,
            Punct(p) if p.as_char() == '>' => {
                depth_angle -= 1;
                if depth_angle == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn next_is(toks: &[proc_macro2::TokenTree], j: usize, chars: &[char]) -> bool {
    matches!(
        toks.get(j + 1),
        Some(proc_macro2::TokenTree::Punct(p)) if chars.contains(&p.as_char())
    )
}

/// Refines a header end by backtracking: among every bare-`>` candidate before
/// `bound_lt` (the enclosing close tag's `<`, which needs no header to find),
/// picks the *last* one whose `start..=cand` slice validates via `is_valid`
/// (full header parse). Falls back to `provisional` (the [`find_header_end`]
/// result) when nothing validates, preserving legacy errors downstream.
///
/// This resolves lone-`>` comparisons (`<case Ok(x) if x > 0>`, `<if a > b>`)
/// that angle-counting cannot: the longest fully-parsing prefix wins, and a
/// `>` inside body RSX never yields a valid header parse (`<` breaks exprs).
pub fn refine_header_end<F>(
    toks: &[proc_macro2::TokenTree],
    start: usize,
    bound_lt: usize,
    provisional: Option<usize>,
    is_valid: F,
) -> Option<usize>
where
    F: Fn(&[proc_macro2::TokenTree]) -> bool,
{
    use proc_macro2::TokenTree::*;
    let mut cands: Vec<usize> = Vec::new();
    let mut j = start + 2;
    while j < bound_lt.min(toks.len()) {
        if matches!(&toks[j], Punct(p) if p.as_char() == '>') {
            // Skip `/>` (self-close) and `=>` — never header ends.
            let prev_op = matches!(&toks[j.saturating_sub(1)], Punct(p) if p.as_char() == '/' || p.as_char() == '=');
            if !prev_op {
                cands.push(j);
            }
        }
        j += 1;
    }
    for cand in cands.into_iter().rev() {
        if is_valid(&toks[start..=cand]) {
            return Some(cand);
        }
    }
    provisional
}

/// `(body_end, close_end)` for the `</tag>` matching the header ending before
/// `from`. Tallies same-tag nesting; `close_end` is the `>` ending the close tag.
pub fn find_close(
    toks: &[proc_macro2::TokenTree],
    from: usize,
    tag: &str,
) -> Option<(usize, usize)> {
    use proc_macro2::TokenTree::*;
    let mut depth = 1i32;
    let mut j = from;
    while j < toks.len() {
        if is_open(toks, j, tag) {
            depth += 1;
            j += 1;
            continue;
        }
        if is_close(toks, j, tag) {
            depth -= 1;
            if depth == 0 {
                let mut k = j;
                while k < toks.len() {
                    if matches!(&toks[k], Punct(p) if p.as_char() == '>') {
                        return Some((j, k));
                    }
                    k += 1;
                }
                return None;
            }
            j += 1;
            continue;
        }
        j += 1;
    }
    None
}

/// First depth-0 `want` ident. `Group` contents are opaque; `<`/`(`/`[`/`{`
/// (and closers) track depth so operators like `>=` (whose `>` is a Punct)
/// never confuse header parsing — callers only invoke this on header slices,
/// where a stray `<`/`>` is already a parse error downstream.
pub fn find_ident_depth0(toks: &[proc_macro2::TokenTree], want: &str) -> Option<usize> {
    use proc_macro2::TokenTree::*;
    let mut depth = 0i32;
    for (idx, t) in toks.iter().enumerate() {
        match t {
            Group(_) => {}
            Punct(p)
                if matches!(p.as_char(), '<' | '(' | '[' | '{') =>
            {
                depth += 1;
            }
            Punct(p)
                if matches!(p.as_char(), '>' | ')' | ']' | '}') =>
            {
                depth -= 1;
            }
            Ident(id) if depth == 0 && id == want => return Some(idx),
            _ => {}
        }
    }
    None
}

/// Strips one `{…}` layer: `key={…}` / header values arrive as a Brace group.
pub fn unbrace(val_ts: &TokenStream2) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = val_ts.clone().into_iter().collect();
    if toks.len() == 1 {
        if let proc_macro2::TokenTree::Group(g) = &toks[0] {
            if g.delimiter() == proc_macro2::Delimiter::Brace {
                return g.stream();
            }
        }
    }
    val_ts.clone()
}

/// Builds a `{placeholder}` splice group for side-table index `n` of `kind`
/// (`"for"` | `"if"` | `"match"`).
pub fn placeholder(kind: &str, n: usize) -> proc_macro2::TokenTree {
    let ident = quote::format_ident!("__aurora_{}_{}", kind, n, span = proc_macro2::Span::call_site());
    proc_macro2::TokenTree::Group(proc_macro2::Group::new(
        proc_macro2::Delimiter::Brace,
        quote::quote! { #ident },
    ))
}

/// Resolves one placeholder ident to expansion tokens.
pub type Resolver<'a> = &'a dyn Fn(&str, usize, proc_macro2::Span) -> syn::Result<TokenStream2>;

/// Walks generated code, replacing every `{__aurora_<kind>_N}` ident via
/// `resolve(kind, n, span)`. Unknown prefixes pass through untouched.
pub fn splice_with(out: TokenStream2, resolve: Resolver) -> syn::Result<TokenStream2> {
    let toks: Vec<proc_macro2::TokenTree> = out.into_iter().collect();
    Ok(splice_stream(toks, resolve)?.into_iter().collect())
}

fn splice_stream(
    toks: Vec<proc_macro2::TokenTree>,
    resolve: Resolver,
) -> syn::Result<Vec<proc_macro2::TokenTree>> {
    use proc_macro2::TokenTree::*;
    let mut out = Vec::with_capacity(toks.len());
    for t in toks {
        match t {
            Ident(id) => {
                let name = id.to_string();
                if let Some(rest) = name.strip_prefix("__aurora_") {
                    let mut parts = rest.splitn(2, '_');
                    // Placeholder shape is `__aurora_<kind>_<n>` where kind has no
                    // underscore; anything else is user code — pass through.
                    let (kind, n) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    if let Ok(n) = n.parse::<usize>() {
                        if matches!(kind, "for" | "if" | "match" | "attr") {
                            let expanded = resolve(kind, n, id.span())?;
                            out.extend(expanded.into_iter());
                            continue;
                        }
                    }
                    out.push(Ident(id));
                } else {
                    out.push(Ident(id));
                }
            }
            Group(g) => {
                let inner: Vec<proc_macro2::TokenTree> = g.stream().into_iter().collect();
                let inner = splice_stream(inner, resolve)?;
                let stream: TokenStream2 = inner.into_iter().collect();
                out.push(Group(proc_macro2::Group::new(g.delimiter(), stream)));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// Whether an identifier names a control-flow placeholder (`__aurora_for_N`,
/// `__aurora_if_N`, `__aurora_match_N`). Attr placeholders (`__aurora_attr_N`)
/// resolve to plain elements and are not control flow.
pub fn is_control_placeholder(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("__aurora_") else {
        return false;
    };
    let Some((kind, n)) = rest.split_once('_') else {
        return false;
    };
    matches!(kind, "for" | "if" | "match") && n.parse::<usize>().is_ok()
}
