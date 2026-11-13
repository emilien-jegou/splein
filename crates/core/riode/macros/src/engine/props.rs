// Single responsibility: `*Props` struct generation, argument binding, and facet-based surface passthrough.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, Type};

struct Field {
    name: syn::Ident,
    ty: Type,
    optional: bool,
    default: Option<TokenStream2>,
    is_children: bool,
}

pub fn expand_props(input: &ItemFn, root_tag: Option<&str>) -> syn::Result<Option<TokenStream2>> {
    let name = &input.sig.ident;
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
        if arg_name == "key" {
            return Err(syn::Error::new_spanned(
                &arg_name,
                "#[component]: `key` is reserved",
            ));
        }

        let mut default = None;
        for attr in &pt.attrs {
            if attr.path().is_ident("prop") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default") {
                        if meta.input.peek(syn::Token![=]) {
                            let value = meta.value()?;
                            let expr: syn::Expr = value.parse()?;
                            default = Some(quote! { #expr });
                        } else {
                            default = Some(quote! { ::std::default::Default::default() });
                        }
                        Ok(())
                    } else {
                        Err(meta.error(
                            "unsupported prop attribute; expected `default` or `default = ...`",
                        ))
                    }
                })?;
            }
        }

        let ty = (*pt.ty).clone();
        let is_children = arg_name == "children";
        let optional = strip_outer_option(&ty).is_some();
        fields.push(Field {
            name: arg_name,
            ty,
            optional,
            default,
            is_children,
        });
    }

    // Synthesize lifetime parameter for props structs if types have elided lifetimes (e.g. `&str`)
    let mut generics = input.sig.generics.clone();
    let mut has_elided = false;
    for f in &mut fields {
        if type_has_elided_lifetime(&f.ty) {
            has_elided = true;
            add_lifetime_to_type(&mut f.ty, &syn::parse_quote!('__riode_a));
        }
    }
    if has_elided {
        let lt_param: syn::GenericParam = syn::parse_quote!('__riode_a);
        generics.params.insert(0, lt_param);
    }

    let public_props = format_ident!("{}Props", type_stem);
    let builder_name = format_ident!("{}PropsBuilder", type_stem);
    let (impl_g, ty_g, where_c) = generics.split_for_impl();

    let struct_fields = fields.iter().map(|f| {
        let (n, t) = (&f.name, &f.ty);
        quote! { pub #n: #t }
    });
    let builder_fields = fields.iter().map(|f| {
        let (n, t) = (&f.name, &f.ty);
        quote! { #n: ::std::option::Option<#t> }
    });
    let builder_inits = fields.iter().map(|f| {
        let n = &f.name;
        quote! { #n: ::std::option::Option::None }
    });

    let setters = fields.iter().map(|f| {
        let (n, t) = (&f.name, &f.ty);
        if f.is_children {
            return quote! { pub fn #n(mut self, v: #t) -> Self { self.#n = Some(v); self } };
        }
        if f.optional {
            let inner = strip_outer_option(t).unwrap();
            quote! {
                pub fn #n(mut self, v: impl ::std::convert::Into<#inner>) -> Self {
                    self.#n = Some(Some(v.into()));
                    self
                }
            }
        } else {
            quote! {
                pub fn #n(mut self, v: impl ::std::convert::Into<#t>) -> Self {
                    self.#n = Some(v.into());
                    self
                }
            }
        }
    });

    let takes: Vec<TokenStream2> = fields
        .iter()
        .map(|f| {
            let (n, comp) = (&f.name, name.to_string());
            if let Some(default_expr) = &f.default {
                if f.optional {
                    quote! { #n: self.#n.unwrap_or_else(|| ::std::option::Option::Some(#default_expr)) }
                } else {
                    quote! { #n: self.#n.unwrap_or_else(|| #default_expr) }
                }
            } else if f.optional {
                quote! { #n: self.#n.unwrap_or(None) }
            } else {
                let msg = format!("{comp}: missing required prop `{n}`");
                quote! { #n: self.#n.expect(#msg) }
            }
        })
        .collect();

    let arg_names = fields.iter().map(|f| {
        let n = &f.name;
        quote! { props.#n }
    });

    // Explicit opt-in: Defaults to empty surface setters unless root primitive is specified!
    let engine = crate::primitives::riode_engine();
    let effective_tag = root_tag
        .or_else(|| {
            let ty_ident = return_type_ident(&input.sig.output)?;
            engine.tag_for_type_name(&ty_ident)
        })
        .or(Some("g")); // Default composite components to group surface facets

    let surface_setters: Vec<TokenStream2> = match effective_tag {
        Some(tag) => engine
            .surface_props_for(tag)
            .into_iter()
            .filter_map(|sp| {
                if fields.iter().any(|f| f.name == sp.name) {
                    return None;
                }
                let ident = format_ident!("{}", sp.name);
                let ty = &sp.arg_type;
                let apply = &sp.apply;
                Some(quote! { pub fn #ident(mut self, #ident: #ty) -> Self { #apply } })
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(Some(quote! {
        pub struct #public_props #ty_g #where_c {
            #(#struct_fields,)*
            pub key: ::std::option::Option<::riode::foundation::Key>,
            pub surface: ::riode::foundation::SurfaceProps,
        }

        pub struct #builder_name #ty_g #where_c {
            #(#builder_fields,)*
            key: ::std::option::Option<::riode::foundation::Key>,
            surface: ::riode::foundation::SurfaceProps,
        }

        impl #impl_g #public_props #ty_g #where_c {
            pub fn builder() -> #builder_name #ty_g {
                #builder_name {
                    #(#builder_inits,)*
                    key: None,
                    surface: ::riode::foundation::SurfaceProps::new(),
                }
            }
        }

        impl #impl_g #builder_name #ty_g #where_c {
            #(#setters)*
            #(#surface_setters)*

            pub fn key(mut self, k: impl Into<::riode::foundation::Key>) -> Self {
                self.key = Some(k.into()); self
            }

            pub fn build(self) -> ::riode::internal::Element {
                let props = #public_props { #(#takes,)* key: self.key, surface: self.surface };
                let el = ::riode::dsl::component::IntoElement::into_element(#name(#(#arg_names),*));
                let el = props.surface.apply(el);
                match props.key {
                    Some(__k) => ::riode::internal::Element::with_key(el, __k),
                    None => el,
                }
            }
        }
    }))
}

fn return_type_ident(output: &syn::ReturnType) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    match &**ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

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

fn type_has_elided_lifetime(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => r.lifetime.is_none() || type_has_elided_lifetime(&r.elem),
        Type::Path(p) => p.path.segments.iter().any(|seg| {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                args.args.iter().any(|arg| match arg {
                    syn::GenericArgument::Type(inner) => type_has_elided_lifetime(inner),
                    syn::GenericArgument::Lifetime(lt) => lt.ident == "_",
                    _ => false,
                })
            } else {
                false
            }
        }),
        Type::Slice(s) => type_has_elided_lifetime(&s.elem),
        Type::Tuple(t) => t.elems.iter().any(type_has_elided_lifetime),
        _ => false,
    }
}

fn add_lifetime_to_type(ty: &mut Type, lt: &syn::Lifetime) {
    match ty {
        Type::Reference(r) => {
            if r.lifetime.is_none() {
                r.lifetime = Some(lt.clone());
            }
            add_lifetime_to_type(&mut r.elem, lt);
        }
        Type::Path(p) => {
            for seg in &mut p.path.segments {
                if let syn::PathArguments::AngleBracketed(args) = &mut seg.arguments {
                    for arg in &mut args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            add_lifetime_to_type(inner, lt);
                        }
                    }
                }
            }
        }
        Type::Slice(s) => add_lifetime_to_type(&mut s.elem, lt),
        Type::Tuple(t) => {
            for elem in &mut t.elems {
                add_lifetime_to_type(elem, lt);
            }
        }
        _ => {}
    }
}
