// Single responsibility: The #[component] attribute, which runs a function body inside the
// ambient reactive hook scope and converts its result to a mountable element.
//
// Single responsibility: procedural macros exposing Aurora's declarative API.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

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
/// The body runs once per mount. Aurora does not re-run a component to animate it: a signal
/// captured in a property binding is re-read every frame, so a component holds no state of
/// its own beyond the signals it allocated.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(
            proc_macro2::TokenStream::from(attr.clone()),
            "#[component] takes no arguments",
        )
        .to_compile_error()
        .into();
    }

    let input = parse_macro_input!(item as ItemFn);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Splits the annotated function into a body function plus a public wrapper.
///
/// The body keeps its original name and return type so its own type checking is untouched,
/// and the wrapper is what callers see: it enters the scope and converts the result.
fn expand(input: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
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
    let mut wrapper_sig = sig.clone();
    wrapper_sig.output = syn::parse_quote!(-> ::aurora::dsl::Element);

    let wrapper = quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#attrs)*
        #vis #wrapper_sig #where_clause {
            ::aurora::dsl::component_scope(|| #body_name(#(#arg_names),*))
        }
    };

    Ok(quote! {
        #body_fn
        #wrapper
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
