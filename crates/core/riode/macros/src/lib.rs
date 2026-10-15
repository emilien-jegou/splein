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
mod preshape;
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
/// A `name = "…"` argument pins the stable hook identity explicitly instead of
/// `module_path!()::Name`, so the component keeps its state across file moves and
/// renames (hot-reload edits change paths; state must not follow them):
///
/// ```ignore
/// #[component(name = "shop::Card")]
/// fn Card() -> impl IntoElement { /* ... */ }
/// ```
///
/// Without arguments the expansion is exactly as before (`::riode` paths, concrete
/// `::riode::internal::Element` return). With `scope`, the wrapper returns `impl IntoElement`
/// instead, resolved from whatever the caller imported.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_scope_arg(attr) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemFn);
    match expand(input, args) {
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

/// Build-time text shaping: `pre_shaped!(font = "f.ttf", text = "...", size = 16.0)`.
///
/// Shapes on the build host and emits
/// `::riode::text::PreShapedText::from_bytes(&[...]).unwrap()`.
/// See `preshape.rs` for contract + limits (single-line rhythm,
/// start-aligned, integer-snapped raster).
#[proc_macro]
pub fn pre_shaped(tokens: TokenStream) -> TokenStream {
    match preshape::pre_shaped_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build-time SVG rasterization: `include_svg!("icon.svg", 64, 64)`.
///
/// Rasterizes on the host at exactly W×H px and emits
/// `::riode::scene::ImageSource::from_static_rgba8(..)`.
#[proc_macro]
pub fn include_svg(tokens: TokenStream) -> TokenStream {
    match preshape::include_svg_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Optional `scope = ::path::to::scope_fn` and/or `name = "identity"` arguments.
///
/// Empty means the defaults (`::riode::internal::component_scope` behavior and
/// `module_path!()::Name` identity). `name` pins the stable hook identity explicitly, so
/// a component keeps its state across file moves and renames — which is exactly what a
/// hot-reload edit cycle does to paths. Anything else is a compile error naming itself.
fn parse_scope_arg(attr: TokenStream) -> syn::Result<ScopeArgs> {
    if attr.is_empty() {
        return Ok(ScopeArgs::default());
    }
    let parsed: ScopeArgs = syn::parse(attr)?;
    Ok(parsed)
}

/// Parsed `scope = ::path` / `name = "…"` assignments, in any order.
#[derive(Default)]
struct ScopeArgs {
    path: Option<syn::Path>,
    name: Option<syn::LitStr>,
}

impl syn::parse::Parse for ScopeArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut out = ScopeArgs::default();
        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            let _eq: syn::Token![=] = input.parse()?;
            if ident == "scope" {
                if out.path.is_some() {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "#[component] takes only one `scope = …`",
                    ));
                }
                out.path = Some(input.parse()?);
            } else if ident == "name" {
                if out.name.is_some() {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "#[component] takes only one `name = \"…\"`",
                    ));
                }
                let lit: syn::LitStr = input.parse()?;
                if lit.value().is_empty() {
                    return Err(syn::Error::new_spanned(
                        lit,
                        "#[component] `name` must be non-empty",
                    ));
                }
                out.name = Some(lit);
            } else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "#[component] takes only `scope = ::path::to::scope_fn` and/or `name = \"identity\"`",
                ));
            }
            if !input.is_empty() {
                let _comma: syn::Token![,] = input.parse()?;
            }
        }
        Ok(out)
    }
}

/// Splits the annotated function into a body function plus a public wrapper.
///
/// The body keeps its original name and return type so its own type checking is untouched,
/// and the wrapper is what callers see: it enters the scope and converts the result.
/// With no `scope` argument the wrapper is byte-identical to the historical expansion;
/// with one, the scope path is swapped and the return becomes `impl IntoElement` (named
/// by the caller's own imports, since the macro cannot know the other engine's type).
fn expand(input: ItemFn, args: ScopeArgs) -> syn::Result<proc_macro2::TokenStream> {
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            input.sig.asyncness,
            "#[component] cannot be applied to an async fn: the body must build an element \
             synchronously",
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
    let custom_scope = args.path.is_some();
    let mut wrapper_sig = sig.clone();
    let scope_call;
    if let Some(path) = args.path {
        wrapper_sig.output = syn::parse_quote!(-> impl IntoElement);
        scope_call = quote! { #path(|| #body_name(#(#arg_names),*)) };
    } else {
        wrapper_sig.output = syn::parse_quote!(-> ::riode::internal::Element);
        // Stable identity: module path + component name, so ordinal hooks
        // survive remounts and rebuilds while call order is unchanged.
        // Siblings disambiguate by child index; modules can't collide.
        // An explicit `name = "…"` pins the identity instead, surviving file moves
        // and renames (hot-reload edits change paths; state must not follow them).
        let identity = match args.name {
            Some(name) => quote! { #name },
            None => quote! {
                ::std::concat!(::std::module_path!(), "::", ::std::stringify!(#name))
            },
        };
        scope_call = quote! { ::riode::internal::component_scope_named(
            #identity,
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

    // P11 — components callable from Rust as `Card::build(…)`. The wrapper `Card(…)`
    // already lives in the value namespace; a *braced* struct `Card { … }` occupies only
    // the type namespace (a unit struct would too and would collide, but a braced one has
    // no value-namespace constructor), so `Card::build(…)` resolves through the type while
    // `Card(…)` and the `<Card …/>` tag (→ `CardProps`) are untouched. The private field
    // blocks `Card { … }` construction — `build`/the fn are the only doors. Skipped for
    // custom-scope components (no Aurora element type to name) and generics.
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
                /// Rust-callable constructor — the same element as the `<#name …/>` tag
                /// and as calling `#name(…)` directly. One widget, three call syntaxes.
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
