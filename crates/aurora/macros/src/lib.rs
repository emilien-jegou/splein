// Single responsibility: The #[component] attribute, which runs a function body inside the
// ambient reactive hook scope and converts its result to a mountable element.
//
// Single responsibility: procedural macros exposing Aurora's declarative API.

//! Procedural macros exposing Aurora's declarative API.

#![feature(proc_macro_diagnostic)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

mod attr_node;
mod flow_util;
mod for_node;
mod if_node;
mod match_node;
mod props;
mod view;

/// Marks a function as a component.
///
/// The body may call the `use_*` hooks with no arguments, because it runs inside the ambient
/// hook scope. Anything the body returns must convert into an element.
///
/// ```ignore
/// #[component]
/// fn Card(open: Signal<bool>) -> impl IntoElement {
///     let t = Transition::spring(Spring::snappy()).on(open.map(|o| if o { 1.0 } else { 0.0 }));
///     group().scale(t)
/// }
/// ```
///
/// The body runs once per mount inside a scope keyed by `module_path!()::Name`,
/// so ordinal hooks survive remounts and rebuilds while call order is
/// unchanged. Aurora does not re-run a component to animate it: a signal
/// captured in a property binding is re-read every frame, so a component holds no state of
/// its own beyond the signals it allocated.
///
/// An alternate engine can reuse the macro by naming its own scope entry point — a
/// function taking the body closure the way `component_scope` does:
///
/// ```ignore
/// #[component(scope = ::my_engine::enter_component)]
/// fn Card() -> impl my_engine::IntoElement { /* ... */ }
/// ```
///
/// Without arguments the expansion is exactly as before (`::aurora` paths, concrete
/// `::aurora::dsl::Element` return). With `scope`, the wrapper returns `impl IntoElement`
/// instead, resolved from whatever the caller imported.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let scope = match parse_scope_arg(attr) {
        Ok(scope) => scope,
        Err(err) => return err.to_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemFn);
    match expand(input, scope) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// RSX over the builder DSL: `<Group width={x}>…</Group>`.
///
/// See `view.rs`. Single root only (Aurora has no fragment node); components
/// resolve to the `{Tag}Props` builder their `#[component]` generated.
#[proc_macro]
pub fn view(tokens: TokenStream) -> TokenStream {
    match view::expand(tokens.into()) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Optional `scope = ::path::to::scope_fn` argument.
///
/// Empty means the default (`::aurora::dsl::component_scope`); anything else must be a
/// single `scope = <path>` assignment.
fn parse_scope_arg(attr: TokenStream) -> syn::Result<Option<syn::Path>> {
    if attr.is_empty() {
        return Ok(None);
    }
    let parsed: ScopeArg = syn::parse(attr)?;
    Ok(Some(parsed.path))
}

/// Parsed `scope = ::path` assignment.
struct ScopeArg {
    path: syn::Path,
}

impl syn::parse::Parse for ScopeArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        if ident != "scope" {
            return Err(syn::Error::new_spanned(
                ident,
                "#[component] takes only `scope = ::path::to::scope_fn`",
            ));
        }
        let _eq: syn::Token![=] = input.parse()?;
        let path: syn::Path = input.parse()?;
        if !input.is_empty() {
            return Err(syn::Error::new(input.span(), "unexpected tokens after scope path"));
        }
        Ok(Self { path })
    }
}

/// Splits the annotated function into a body function plus a public wrapper.
///
/// The body keeps its original name and return type so its own type checking is untouched,
/// and the wrapper is what callers see: it enters the scope and converts the result.
/// With no `scope` argument the wrapper is byte-identical to the historical expansion;
/// with one, the scope path is swapped and the return becomes `impl IntoElement` (named
/// by the caller's own imports, since the macro cannot know the other engine's type).
fn expand(input: ItemFn, scope: Option<syn::Path>) -> syn::Result<proc_macro2::TokenStream> {
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            input.sig.asyncness,
            "#[component] cannot be applied to an async fn: the body must build an element \
             synchronously",
        ));
    }

    let name = input.sig.ident.clone();
    let body_name = syn::Ident::new(&format!("__aurora_component_body_{name}"), name.span());
    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let where_clause = &input.sig.generics.where_clause;
    let arg_names = call_arg_names(&input);

    // The body function keeps the declared return type, which is what lets an `impl
    // IntoElement` signature keep working and still be checked by the compiler. Only the
    // name changes; syn's `Signature` already carries its own `fn` token.
    let mut body_sig = sig.clone();
    body_sig.ident = body_name.clone();
    let body_fn: ItemFn = syn::parse2(quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#attrs)*
        #body_sig #block
    })?;

    // The wrapper replaces the declared return type with the one thing the scope actually
    // yields: a mountable element. The body's own `impl IntoElement` return is still checked
    // by the compiler, inside the body function, so the opaque type is verified rather than
    // erased. syn's `Signature` carries its own `fn` token, so it is emitted whole.
    //
    // A custom scope cannot name the other engine's element type, so the return falls back
    // to `impl IntoElement`, resolved from the caller's imports.
    // Typed props for view!: additive and non-breaking; custom-scope components
    // (other engines) skip it since the builder names Aurora paths.
    let custom_scope = scope.is_some();
    let mut wrapper_sig = sig.clone();
    let scope_call;
    if let Some(path) = scope {
        wrapper_sig.output = syn::parse_quote!(-> impl IntoElement);
        scope_call = quote! { #path(|| #body_name(#(#arg_names),*)) };
    } else {
        wrapper_sig.output = syn::parse_quote!(-> ::aurora::dsl::Element);
        // Stable identity: module path + component name, so ordinal hooks
        // survive remounts and rebuilds while call order is unchanged.
        // Siblings disambiguate by child index; modules can't collide.
        scope_call = quote! { ::aurora::dsl::component_scope_named(
            ::std::concat!(::std::module_path!(), "::", ::std::stringify!(#name)),
            ::std::file!(),
            || #body_name(#(#arg_names),*)
        ) };
    }

    let wrapper = quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#attrs)*
        #vis #wrapper_sig #where_clause {
            #scope_call
        }
    };

    // Typed props for view!: additive and non-breaking; custom-scope components
    // (other engines) skip it since the builder names Aurora paths.
    let props_tokens = if custom_scope {
        quote! {}
    } else {
        props::expand_props(&input)?.unwrap_or_else(|| quote! {})
    };

    Ok(quote! {
        #body_fn
        #wrapper
        #props_tokens
    })
}

/// Collects the wrapper's parameter names, so it forwards exactly what it received.
///
/// Patterns are supported through binding modes: `mut x` and `x: T` both bind a name.
fn call_arg_names(input: &ItemFn) -> Vec<proc_macro2::TokenStream> {
    input
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat) => strip_ref_pat(&pat.pat),
            syn::FnArg::Receiver(_) => quote! { _self },
        })
        .collect()
}

/// Reduces a parameter pattern to the name it binds, dropping `ref` and `mut` markers.
fn strip_ref_pat(pat: &syn::Pat) -> proc_macro2::TokenStream {
    match pat {
        syn::Pat::Ident(ident) => {
            let name = &ident.ident;
            quote! { #name }
        }
        other => {
            let _ = other;
            quote! { _ }
        }
    }
}
