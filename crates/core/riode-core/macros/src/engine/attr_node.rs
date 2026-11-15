// Responsibility: Nested RSX in attribute position — `<Panel head={<Strip …/>}>`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::engine::flow::FlowTables;
use crate::engine::flow_util;

/// Extracts top-level `<Tag>` headers' nested-RSX attr values, replacing each
/// with a `{__riode_attr_N}` placeholder.
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
                        quote! { #ident },
                    ));
                }
            }
        }
        j += 1;
    }
}

fn group_starts_with_tag(stream: TokenStream2) -> bool {
    let toks: Vec<proc_macro2::TokenTree> = stream.into_iter().collect();
    use proc_macro2::TokenTree::*;
    matches!(toks.first(), Some(Punct(p)) if p.as_char() == '<')
        && matches!(toks.get(1), Some(Ident(_)))
        && !matches!(toks.get(2), Some(Punct(p)) if p.as_char() == '/')
}

pub fn expand_attr(raw: &TokenStream2, outer: &FlowTables) -> syn::Result<TokenStream2> {
    
    let mut flow = FlowTables::default();
    let rewritten = flow.extract(raw.clone());
    flow.warn();
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        out = quote! { #out #t; };
    }
    let single = match nodes.as_slice() {
        [single] => single,
        _ => {
            return Ok(quote! {
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
                return Ok(quote! {
                    #out ::std::compile_error!(
                        "view!: control nodes (`<for>`, `<if>`, `<match>`) must be direct children — hoist them out of attributes into children position"
                    )
                });
            }
            match crate::engine::element_expr(el, &flow) {
                Ok(e) => quote! { (#e) },
                Err(err) => {
                    let compile = err.to_compile_error();
                    quote! { #out #compile }
                }
            }
        }
        _ => {
            return Ok(quote! {
                #out ::std::compile_error!(
                    "view!: an attribute holds one nested element — `<Panel head={<Strip …/>}>`; anything else goes in `{…}` as Rust"
                )
            });
        }
    };
    flow.splice_attr_ref(body, outer)
}
