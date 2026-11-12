// Single responsibility: Procedural macro entry points exposing Riode's declarative RSX and component model.

#![feature(proc_macro_diagnostic)]

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{parse_macro_input, ItemFn};

mod engine;
mod preshape;
mod primitives;

/// RSX over the builder DSL: `<g w="fill">…</g>`.
#[proc_macro]
pub fn view(tokens: TokenStream) -> TokenStream {
    let rx = primitives::riode_engine();
    match engine::view_expand(tokens.into(), &rx) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Marks a function as a component with reactive hook scope and typed props builder.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_scope_arg(attr) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemFn);
    match expand_component(input, args) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Build-time text shaping: `pre_shaped!(font = "f.ttf", text = "...", size = 16.0)`.
#[proc_macro]
pub fn pre_shaped(tokens: TokenStream) -> TokenStream {
    match preshape::pre_shaped_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build-time SVG rasterization: `include_svg!("icon.svg", 64, 64)`.
#[proc_macro]
pub fn include_svg(tokens: TokenStream) -> TokenStream {
    match preshape::include_svg_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build-time image decode + resample: `include_image!("icon.png", 64, 64)`.
#[proc_macro]
pub fn include_image(tokens: TokenStream) -> TokenStream {
    match preshape::include_image_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[derive(Default)]
struct ScopeArgs {
    path: Option<syn::Path>,
    name: Option<syn::LitStr>,
    root: Option<String>,
}

fn parse_scope_arg(attr: TokenStream) -> syn::Result<ScopeArgs> {
    if attr.is_empty() {
        return Ok(ScopeArgs::default());
    }

    let parser = |input: syn::parse::ParseStream| -> syn::Result<ScopeArgs> {
        let mut out = ScopeArgs::default();

        // 1. Shorthand positional string literal: #[component("text")]
        if input.peek(syn::LitStr) {
            let lit: syn::LitStr = input.parse()?;
            out.root = Some(lit.value());
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }

        // 2. Named arguments or flags
        while !input.is_empty() {
            if input.peek(syn::Ident) {
                let ident: syn::Ident = input.parse()?;
                if ident == "surface" {
                    if input.peek(syn::Token![=]) {
                        input.parse::<syn::Token![=]>()?;
                        let lit: syn::LitStr = input.parse()?;
                        out.root = Some(lit.value());
                    } else {
                        out.root = Some("g".to_string());
                    }
                } else if ident == "root" || ident == "type" {
                    input.parse::<syn::Token![=]>()?;
                    let lit: syn::LitStr = input.parse()?;
                    out.root = Some(lit.value());
                } else if ident == "name" {
                    input.parse::<syn::Token![=]>()?;
                    let lit: syn::LitStr = input.parse()?;
                    out.name = Some(lit);
                } else if ident == "scope" {
                    input.parse::<syn::Token![=]>()?;
                    let path: syn::Path = input.parse()?;
                    out.path = Some(path);
                } else {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "unknown option, expected `\"tag\"`, `surface`, `name = ...`, `scope = ...`",
                    ));
                }
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else {
                break;
            }
        }

        Ok(out)
    };

    parser.parse(attr)
}

fn expand_component(input: ItemFn, args: ScopeArgs) -> syn::Result<proc_macro2::TokenStream> {
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            input.sig.asyncness,
            "#[component] cannot be async",
        ));
    }

    let name = input.sig.ident.clone();
    let body_name = syn::Ident::new(&format!("__riode_component_body_{name}"), name.span());
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let where_clause = &input.sig.generics.where_clause;
    let arg_names = call_arg_names(&input);

    let mut body_sig = sig.clone();
    body_sig.ident = body_name.clone();
    let body_fn: ItemFn = syn::parse2(quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#attrs)*
        #body_sig #block
    })?;

    let custom_scope = args.path.is_some();
    let mut wrapper_sig = sig.clone();
    let scope_call = if let Some(path) = args.path {
        wrapper_sig.output = syn::parse_quote!(-> impl IntoElement);
        quote! { #path(|| #body_name(#(#arg_names),*)) }
    } else {
        wrapper_sig.output = syn::parse_quote!(-> ::riode::internal::Element);
        let identity = match args.name {
            Some(n) => quote! { #n },
            None => quote! { ::std::concat!(::std::file!(), "::", ::std::stringify!(#name)) },
        };
        quote! { ::riode::internal::component_scope_named(#identity, ::std::file!(), || #body_name(#(#arg_names),*)) }
    };

    let wrapper = quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#attrs)*
        #vis #wrapper_sig #where_clause {
            #scope_call
        }
    };

    let root_tag = args.root.as_deref();
    let props_tokens = if custom_scope {
        quote! {}
    } else {
        engine::props::expand_props(&input, root_tag.as_deref())?.unwrap_or_else(|| quote! {})
    };

    let build_tokens = if custom_scope || !input.sig.generics.params.is_empty() {
        quote! {}
    } else {
        let inputs = &sig.inputs;
        quote! {
            #[doc(hidden)]
            #[allow(non_snake_case)]
            #[allow(dead_code)]
            #vis struct #name { _riode_build: () }

            impl #name {
                #[allow(clippy::too_many_arguments)]
                #vis fn build(#inputs) -> ::riode::internal::Element {
                    #name(#(#arg_names),*)
                }
            }
        }
    };

    Ok(quote! {
        #body_fn
        #wrapper
        #props_tokens
        #build_tokens
    })
}

fn call_arg_names(input: &ItemFn) -> Vec<proc_macro2::TokenStream> {
    input
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat) => match &*pat.pat {
                syn::Pat::Ident(ident) => {
                    let n = &ident.ident;
                    quote! { #n }
                }
                _ => quote! { _ },
            },
            syn::FnArg::Receiver(_) => quote! { _self },
        })
        .collect()
}
