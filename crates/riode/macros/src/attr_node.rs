// Single responsibility: nested RSX in attribute position — `<Panel head={<Strip …/>}>`.
//
/// Attribute values are the one place rstml never recovers: `{…}` after `=`
/// must parse as Rust, so `{<Strip/>}` dies with `expected '>'` before any
/// fallback runs. This pre-pass (running after the flow extracts, before
/// rstml) finds `=` + brace-group-starting-with-`<Tag`, stashes the inner
/// RSX, and substitutes `{__riode_attr_N}` — a plain path expression rstml
/// accepts. Expansion is DEFERRED to the final splice (see
/// [`crate::view::FlowTables`]): stored raw tokens expand then, so nested
/// attr references (strictly smaller indices — inner stashes first) resolve
/// in one pass with no ordering hazard.
///
/// Scope: only brace groups *starting* with `<Tag` are touched. Rust blocks
/// containing `<` deeper (`{a < b}`, `{foo(head={<X/>})}`) are left alone —
/// RSX inside Rust expressions inside RSX is out of scope by design.
use proc_macro2::TokenStream as TokenStream2;

use crate::flow_util;

/// Extracts top-level `<Tag>` headers' nested-RSX attr values, replacing each
/// with a `{__riode_attr_N}` placeholder. Headers already rewritten by the
/// flow extracts (`{__riode_for_N}` etc.) are opaque groups and pass through.
pub fn extract(tokens: TokenStream2, attr_raw: &mut Vec<TokenStream2>) -> TokenStream2 {
    let toks: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<proc_macro2::TokenTree> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if is_any_open(&toks, i) {
            let start = i;
            let Some(header_end) = flow_util::find_header_end(&toks, start) else {
                out.extend(toks[start..].iter().cloned());
                break;
            };
            let mut header: Vec<proc_macro2::TokenTree> = toks[start..=header_end].to_vec();
            process_header(&mut header, attr_raw);
            out.extend(header);
            i = header_end + 1;
        } else {
            out.push(toks[i].clone());
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// `<` + identifier, not a close tag. Tag name is irrelevant here: flow nodes
/// (`for`/`if`/`match`) already became placeholders upstream, and anything
/// else is either a real tag (process) or a later hard error (skip safely —
/// only headers matter, and a broken header still ends at `>`).
fn is_any_open(toks: &[proc_macro2::TokenTree], i: usize) -> bool {
    use proc_macro2::TokenTree::*;
    matches!(
        toks.get(i),
        Some(Punct(p)) if p.as_char() == '<'
    ) && matches!(toks.get(i + 1), Some(Ident(_)))
        && !matches!(
            toks.get(i + 2),
            Some(Punct(p)) if p.as_char() == '/'
        )
}

/// Rewrites one header's `=` + RSX-group values in place. Groups are opaque
/// tokens, so nesting *inside* a value (`a={<B c={<D/>}/>}`) is handled by
/// recursing [`extract`] into the value first — strictly smaller input, so
/// stashed indices decrease inward and splice-time recursion terminates.
fn process_header(header: &mut Vec<proc_macro2::TokenTree>, attr_raw: &mut Vec<TokenStream2>) {
    use proc_macro2::TokenTree::*;
    let mut j = 0;
    while j + 1 < header.len() {
        let is_assign = matches!(&header[j], Punct(p) if p.as_char() == '=')
            && !matches!(&header[j.saturating_sub(1)], Punct(p) if p.as_char() == '=' || p.as_char() == '!' || p.as_char() == '<' || p.as_char() == '>');
        if is_assign {
            if let Some(Group(g)) = header.get(j + 1) {
                if g.delimiter() == proc_macro2::Delimiter::Brace
                    && group_starts_with_tag(g.stream())
                {
                    let inner: TokenStream2 = extract(g.stream(), attr_raw);
                    let n = attr_raw.len();
                    attr_raw.push(inner);
                    let ident = quote::format_ident!(
                        "__riode_attr_{}",
                        n,
                        span = proc_macro2::Span::call_site()
                    );
                    header[j + 1] = Group(proc_macro2::Group::new(
                        proc_macro2::Delimiter::Brace,
                        quote::quote! { #ident },
                    ));
                }
            }
        }
        j += 1;
    }
}

/// A brace group's stream starting with `<Tag` (open, not close).
fn group_starts_with_tag(stream: TokenStream2) -> bool {
    let toks: Vec<proc_macro2::TokenTree> = stream.into_iter().collect();
    use proc_macro2::TokenTree::*;
    matches!(toks.first(), Some(Punct(p)) if p.as_char() == '<')
        && matches!(toks.get(1), Some(Ident(_)))
        && !matches!(toks.get(2), Some(Punct(p)) if p.as_char() == '/')
}

/// Expands one stashed attr body to a final element expression, resolving
/// nested control flow with fresh local tables and nested attr references by
/// recursing into `outer`.
pub fn expand_attr(
    raw: &TokenStream2,
    outer: &crate::view::FlowTables,
) -> syn::Result<TokenStream2> {
    let mut flow = crate::view::FlowTables::default();
    let rewritten = flow.extract(raw.clone());
    flow.warn();
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut out = quote::quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        out = quote::quote! { #out #t; };
    }
    let single = match nodes.as_slice() {
        [single] => single,
        _ => {
            return Ok(quote::quote! {
                #out ::std::compile_error!(
                    "view!: an attribute holds one nested element — `<Panel head={<Strip …/>}>`; anything else goes in `{…}` as Rust"
                )
            });
        }
    };
    let body = match single {
        rstml::node::Node::Element(el) => {
            let tag = el.name().to_string();
            if ["for", "if", "match", "else", "case"]
                .iter()
                .any(|k| tag.ends_with(k))
            {
                return Ok(quote::quote! {
                    #out ::std::compile_error!(
                        "view!: control nodes (`<for>`, `<if>`, `<match>`) must be direct children — hoist them out of attributes into children position"
                    )
                });
            }
            match crate::view::element_expr(el, &flow) {
                Ok(e) => quote::quote! { (#e) },
                Err(err) => {
                    let compile = err.to_compile_error();
                    quote::quote! { #out #compile }
                }
            }
        }
        _ => {
            return Ok(quote::quote! {
                #out ::std::compile_error!(
                    "view!: an attribute holds one nested element — `<Panel head={<Strip …/>}>`; anything else goes in `{…}` as Rust"
                )
            });
        }
    };
    // Local flow nodes (control flow nested in the attr) resolve here;
    // `attr` references recurse into the outer tables (smaller indices only).
    flow.splice_attr_ref(body, outer)
}
