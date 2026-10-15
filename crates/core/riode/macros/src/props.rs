// Single responsibility: `*Props` struct + builder generation for #[component].
//
// Non-breaking: the plain-arg wrapper keeps working. This emits alongside it:
// `{Name}Props` (all fields) + `{Name}PropsBuilder` (typed setters) whose
// `build()` collects the fields and calls the wrapper. `view!` targets the
// builder; hand-written call sites keep calling the wrapper directly.
//
// Inference (no attributes needed):
// - outer `Option<T>`          → omissible, missing means `None`
// - `Prop<T>` / `DynamicProp<T>` → reactive setter (`impl IntoPropValue<T>`)
// - `Option<Prop<T>>`          → omissible + reactive
// - `children: Vec<Element>`   → children slot (filled by nested RSX)
// - `#[prop(default)]` / `#[prop(default = expr)]` → omitted means the default
// - anything else              → required, setter takes `impl Into<T>`

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, Type};

/// A `#[prop(...)]` hint on one component argument.
enum PropDefaultAttr {
    None,
    DefaultTrait,
    DefaultExpr(syn::Expr),
}

fn prop_default_attr(attrs: &[syn::Attribute]) -> syn::Result<PropDefaultAttr> {
    for attr in attrs {
        if !attr.path().is_ident("prop") {
            continue;
        }
        let mut is_default = false;
        let mut expr: Option<syn::Expr> = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                is_default = true;
                if meta.input.peek(syn::Token![=]) {
                    let _eq: syn::Token![=] = meta.input.parse()?;
                    expr = Some(meta.input.parse()?);
                }
                Ok(())
            } else {
                Err(meta.error("unknown #[prop] option; expected `default` or `default = ...`"))
            }
        })?;
        if is_default {
            match expr {
                Some(e) => return Ok(PropDefaultAttr::DefaultExpr(e)),
                None => return Ok(PropDefaultAttr::DefaultTrait),
            }
        }
    }
    Ok(PropDefaultAttr::None)
}

/// Last path segment identifier of a type, if it is a plain path.
#[allow(dead_code)]
fn last_segment_ident(ty: &Type) -> Option<String> {
    if let Type::Path(p) = ty {
        p.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

/// Strips one outer `Option<...>` layer, returning the inner type.
fn strip_outer_option(ty: &Type) -> Option<Type> {
    if let Type::Path(p) = ty {
        let seg = p.path.segments.last()?;
        if seg.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner.clone());
                }
            }
        }
    }
    None
}

/// Whether the type is (possibly option-wrapped) `Prop<T>` / `DynamicProp<T>`,
/// returning the reactive value type `T`.
fn reactive_inner(ty: &Type) -> Option<Type> {
    if let Type::Path(p) = ty {
        let seg = p.path.segments.last()?;
        if seg.ident == "Prop" || seg.ident == "DynamicProp" {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner.clone());
                }
            }
        }
    }
    None
}

struct Field {
    name: syn::Ident,
    ty: Type,
    optional: bool,
    reactive: Option<Type>,
    default: PropDefaultAttr,
    is_children: bool,
}

/// Emits `{Name}Props` + builder. Argless components get an empty props
/// struct, so `<Tag />` works uniformly without a special case.
pub fn expand_props(input: &ItemFn) -> syn::Result<Option<TokenStream2>> {
    let name = &input.sig.ident;
    // Type names are CamelCase even when the component fn is snake_case
    // (`item` → `ItemProps`, not `itemProps`).
    let type_stem = {
        let s = name.to_string();
        let mut c = s.chars();
        match c.next() {
            Some(f) => format!("{}{}", f.to_uppercase(), c.as_str()),
            None => s,
        }
    };

    let mut fields = Vec::new();
    for arg in &input.sig.inputs {
        let FnArg::Typed(pt) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "#[component] methods (self) are not supported",
            ));
        };
        let Pat::Ident(pat) = &*pt.pat else {
            return Err(syn::Error::new_spanned(
                &pt.pat,
                "#[component] arguments must be simple bindings (`name: Type`)",
            ));
        };
        let arg_name = pat.ident.clone();
        // `key` is reserved for reconciliation identity (P3): the macro
        // routes the call site's `key=` into a hidden builder field; a
        // parameter of the same name would be a second, unrelated meaning.
        if arg_name == "key" {
            return Err(syn::Error::new_spanned(
                &arg_name,
                "#[component]: `key` is reserved for reconciliation identity — remove the parameter and pass `key=` at the call site (`<Tag key={…}>`)",
            ));
        }
        // `#[prop(...)]` hints live on the argument itself.
        let default = prop_default_attr(&pt.attrs)?;
        let ty = (*pt.ty).clone();
        let is_children = arg_name == "children";
        let optional = strip_outer_option(&ty).is_some();
        let value_ty = strip_outer_option(&ty).unwrap_or_else(|| ty.clone());
        let reactive = reactive_inner(&value_ty);
        if matches!(
            default,
            PropDefaultAttr::DefaultTrait | PropDefaultAttr::DefaultExpr(_)
        ) && optional
        {
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "#[component] `#[prop(default)]` contradicts `Option<...>`: omission cannot be both a default and `None`",
            ));
        }
        fields.push(Field {
            name: arg_name,
            ty,
            optional,
            reactive,
            default,
            is_children,
        });
    }

    let public_props = format_ident!("{}Props", type_stem);
    let builder_name = format_ident!("{}PropsBuilder", type_stem);
    let (impl_g, ty_g, where_c) = input.sig.generics.split_for_impl();

    let struct_fields = fields.iter().map(|f| {
        let n = &f.name;
        let t = &f.ty;
        quote! { pub #n: #t }
    });
    // Builder stores every field as Option<field_ty>; required-ness is enforced in build().
    let builder_fields = fields.iter().map(|f| {
        let n = &f.name;
        let t = &f.ty;
        quote! { #n: ::std::option::Option<#t> }
    });
    let builder_inits = fields.iter().map(|f| {
        let n = &f.name;
        quote! { #n: ::std::option::Option::None }
    });
    let setters = fields.iter().map(|f| {
        let n = &f.name;
        let t = &f.ty;
        if f.is_children {
            // Children arrive as a Vec from view!; accept it directly.
            return quote! {
                pub fn #n(mut self, v: #t) -> Self {
                    self.#n = ::std::option::Option::Some(v);
                    self
                }
            };
        }
        if let Some(inner) = &f.reactive {
            if f.optional {
                quote! {
                    pub fn #n(mut self, v: impl ::riode::reactive::IntoPropValue<#inner>) -> Self {
                        self.#n = ::std::option::Option::Some(
                            ::std::option::Option::Some(::riode::reactive::IntoPropValue::into_prop_value(v))
                        );
                        self
                    }
                }
            } else {
                quote! {
                    pub fn #n(mut self, v: impl ::riode::reactive::IntoPropValue<#inner>) -> Self {
                        self.#n = ::std::option::Option::Some(::riode::reactive::IntoPropValue::into_prop_value(v));
                        self
                    }
                }
            }
        } else if f.optional {
            let inner = strip_outer_option(t).unwrap_or_else(|| t.clone());
            quote! {
                pub fn #n(mut self, v: impl ::std::convert::Into<#inner>) -> Self {
                    self.#n = ::std::option::Option::Some(::std::option::Option::Some(v.into()));
                    self
                }
            }
        } else {
            quote! {
                pub fn #n(mut self, v: impl ::std::convert::Into<#t>) -> Self {
                    self.#n = ::std::option::Option::Some(v.into());
                    self
                }
            }
        }
    });
    let takes: Vec<TokenStream2> = fields
        .iter()
        .map(|f| {
            let n = &f.name;
            let comp = name.to_string();
            match &f.default {
                PropDefaultAttr::DefaultTrait => quote! {
                    #n: self.#n.unwrap_or_else(|| ::riode::reactive::PropDefault::prop_default())
                },
                PropDefaultAttr::DefaultExpr(e) => quote! { #n: self.#n.unwrap_or_else(|| #e) },
                PropDefaultAttr::None if f.optional => {
                    quote! { #n: self.#n.unwrap_or(::std::option::Option::None) }
                }
                PropDefaultAttr::None => {
                    let msg = format!("{comp}: missing required prop `{}`", n);
                    quote! { #n: self.#n.expect(#msg) }
                }
            }
        })
        .collect();
    let arg_names: Vec<TokenStream2> = fields
        .iter()
        .map(|f| {
            let n = &f.name;
            quote! { props.#n }
        })
        .collect();

    Ok(Some(quote! {
        /// Typed props for [`#name`]. Prefer building through `view!`.
        pub struct #public_props #ty_g #where_c {
            #(#struct_fields,)*
            /// Reconciliation identity from the call site's `key=` — applied
            /// to the element the body returns; never a user parameter.
            pub key: ::std::option::Option<::riode::foundation::Key>,
        }

        /// Builder for [`#public_props`]; `view!` targets this.
        pub struct #builder_name #ty_g #where_c {
            #(#builder_fields,)*
            key: ::std::option::Option<::riode::foundation::Key>,
        }

        impl #impl_g #public_props #ty_g #where_c {
            pub fn builder() -> #builder_name #ty_g {
                #builder_name { #(#builder_inits,)* key: ::std::option::Option::None }
            }
        }

        impl #impl_g #builder_name #ty_g #where_c {
            #(#setters)*
            /// Reconciliation identity, reserved on every component tag.
            pub fn key(mut self, k: impl ::std::convert::Into<::riode::foundation::Key>) -> Self {
                self.key = ::std::option::Option::Some(::std::convert::Into::into(k));
                self
            }
            pub fn build(self) -> ::riode::dsl::Element {
                let props = #public_props { #(#takes,)* key: self.key };
                let __riode_el = #name(#(#arg_names),*);
                match props.key {
                    ::std::option::Option::Some(__k) => {
                        ::riode::dsl::Element::with_key(__riode_el, __k)
                    }
                    ::std::option::Option::None => __riode_el,
                }
            }
        }
    }))
}
