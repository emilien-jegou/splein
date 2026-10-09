// Single responsibility: the `view!` RSX macro — XML-ish syntax over the builder DSL.
//
// `<Group width={x} fill={c}>…</Group>` expands to `group().width(x).fill(c)`
// chains; `<Page title="t">…</Page>` expands to the `PageProps` builder that
// #[component] generated; `"lit"` / `{expr}` children become text / spliced
// elements. Unknown builder methods fail in plain Rust, so diagnostics stay
// native. One rule per position, no prefixes beyond `on:`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use rstml::node::{
    Infallible, KVAttributeValue, KeyedAttribute, KeyedAttributeValue, Node,
    NodeAttribute, NodeElement, NodeName,
};

/// Concrete RSTML node types (no custom nodes).
type Element = NodeElement<Infallible>;
type ViewNode = Node<Infallible>;

/// Primitives that map to `::aurora::dsl::<lower>()` builder constructors.
fn primitive_ctor(tag: &str) -> Option<TokenStream2> {
    match tag {
        "Group" => Some(quote! { ::aurora::dsl::group() }),
        "Text" => None, // handled specially: content is a constructor arg
        "Image" => Some(quote! { ::aurora::dsl::image() }),
        "Svg" => Some(quote! { ::aurora::dsl::svg() }),
        "Custom" => None, // `custom()` needs a Primitive arg; require it via children? reject
        _ => None,
    }
}

fn is_primitive(tag: &str) -> bool {
    matches!(tag, "Group" | "Text" | "Image" | "Svg")
}

fn node_name_string(name: &NodeName) -> String {
    match name {
        NodeName::Path(p) => p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        other => other.to_string().trim().to_string(),
    }
}

/// `on:click` → `on_click`; `foo-bar` → `foo_bar`; else identity.
fn attr_method(key: &str) -> String {
    if let Some(ev) = key.strip_prefix("on:") {
        format!("on_{}", ev.replace(['-', ':'], "_"))
    } else {
        key.replace(['-', ':'], "_")
    }
}

fn attr_value_tokens(attr: &KeyedAttribute) -> TokenStream2 {
    match &attr.possible_value {
        KeyedAttributeValue::Binding(_) => {
            // `attr(args)` closure-arg form: not meaningful for Aurora props.
            quote! { ::std::compile_error!("view!: `attr(...)` closure args are not supported; pass a value") }
        }
        KeyedAttributeValue::Value(v) => match &v.value {
            KVAttributeValue::Expr(syn::Expr::Block(b))
                if b.attrs.is_empty() && b.label.is_none() =>
            {
                strip_braces(&b.block)
            }
            KVAttributeValue::Expr(e) => quote! { #e },
            KVAttributeValue::InvalidBraced(b) => quote! { #b },
        },
        KeyedAttributeValue::None => {
            // Bare `<X flag>` means boolean true.
            quote! { true }
        }
    }
}

/// Strips one brace layer off `{expr}` when the block is a single bare
/// expression, so codegen doesn't trip `unused_braces`. Multi-statement
/// blocks keep their braces (they're real blocks, not parenthesization).
fn strip_braces(block: &syn::Block) -> TokenStream2 {
    use quote::ToTokens;
    if block.stmts.len() == 1 {
        if let Some(syn::Stmt::Expr(e, None)) = block.stmts.first() {
            return e.to_token_stream();
        }
    }
    block.to_token_stream()
}

fn children_vec(children: &[ViewNode]) -> syn::Result<TokenStream2> {
    let mut items = Vec::new();
    for c in children {
        items.push(child_expr(c)?);
    }
    Ok(quote! { ::std::vec![#(#items),*] })
}

fn child_expr(node: &ViewNode) -> syn::Result<TokenStream2> {
    match node {
        Node::Element(el) => {
            let e = element_expr(el)?;
            Ok(quote! { ::aurora::dsl::IntoElement::into_element(#e) })
        }
        Node::Text(t) => {
            let s = t.value_string();
            Ok(quote! { ::aurora::dsl::IntoElement::into_element(::aurora::dsl::text(#s)) })
        }
        Node::Block(b) => {
            // `{expr}` splices anything IntoElement: signals, strings, closures,
            // nested Elements, Vec<Element> (children passthrough).
            let inner = match b {
                rstml::node::NodeBlock::ValidBlock(block) => strip_braces(block),
                other => quote! { #other },
            };
            Ok(quote! { ::aurora::dsl::view_child(#inner) })
        }
        Node::Fragment(f) => {
            let kids = children_vec(&f.children)?;
            Ok(quote! { ::aurora::dsl::view_child(#kids) })
        }
        Node::Comment(_) | Node::Doctype(_) => Err(syn::Error::new_spanned(
            node,
            "view!: HTML comments/doctype have no Aurora equivalent",
        )),
        Node::RawText(_) => Err(syn::Error::new_spanned(
            node,
            "view!: raw text blocks are not supported; use a quoted string",
        )),
        Node::Custom(_) => Err(syn::Error::new_spanned(
            node,
            "view!: unreachable custom node",
        )),
    }
}

fn apply_attrs(base: TokenStream2, el: &Element) -> syn::Result<TokenStream2> {
    let mut out = base;
    for a in el.attributes() {
        match a {
            NodeAttribute::Block(_) => {
                return Err(syn::Error::new_spanned(
                    a,
                    "view!: spread attributes are not supported; name each prop",
                ));
            }
            NodeAttribute::Attribute(attr) => {
                let key = node_name_string(&attr.key);
                let method = format_ident!("{}", attr_method(&key));
                let val = attr_value_tokens(&attr.clone());
                out = quote! { #out.#method(#val) };
            }
        }
    }
    Ok(out)
}

fn element_expr(el: &Element) -> syn::Result<TokenStream2> {
    let tag = node_name_string(el.name());
    // Reject namespaced/lowercase tags early with a helpful message.
    if tag.contains("::") || tag.chars().next().is_some_and(|c| c.is_lowercase()) {
        return Err(syn::Error::new_spanned(
            el.name(),
            format!(
                "view!: unknown tag `{tag}`. Primitives are Group/Text/Image/Svg (PascalCase); anything else must be a #[component] in scope"
            ),
        ));
    }
    if tag == "Text" {
        return text_expr(el);
    }
    if tag == "Custom" {
        return Err(syn::Error::new_spanned(
            el.name(),
            "view!: <Custom> needs a Primitive value — build it with `custom(...)` in plain Rust instead",
        ));
    }
    if is_primitive(&tag) {
        let ctor = primitive_ctor(&tag).unwrap();
        let mut out = apply_attrs(ctor, el)?;
        if !el.children.is_empty() {
            let kids = children_vec(&el.children)?;
            out = quote! { #out.children(#kids) };
        }
        return Ok(out);
    }
    // Component: `#[component]` generated `{Tag}Props` + builder in scope.
    // `build()` already calls the wrapper, so the chain alone is the element.
    let props = format_ident!("{}Props", tag);
    let mut chain = quote! { #props::builder() };
    for a in el.attributes() {
        match a {
            NodeAttribute::Block(_) => {
                return Err(syn::Error::new_spanned(
                    a,
                    "view!: spread attributes are not supported; name each prop",
                ));
            }
            NodeAttribute::Attribute(attr) => {
                let key = node_name_string(&attr.key);
                if key == "children" {
                    return Err(syn::Error::new_spanned(
                        &attr.key,
                        "view!: pass children as nested RSX, not a `children=` attribute",
                    ));
                }
                let method = format_ident!("{}", attr_method(&key));
                let val = attr_value_tokens(&attr.clone());
                chain = quote! { #chain.#method(#val) };
            }
        }
    }
    if !el.children.is_empty() {
        let kids = children_vec(&el.children)?;
        chain = quote! { #chain.children(#kids) };
    }
    Ok(quote! { #chain.build() })
}

/// `<Text …>children</Text>`: content is a constructor argument, not an attr.
fn text_expr(el: &Element) -> syn::Result<TokenStream2> {
    // Optional `content=` attr plus at most single-child content; mixing both is an error.
    let mut content_attr: Option<TokenStream2> = None;
    let mut base = quote! { ::aurora::dsl::text };
    let mut modifiers: Vec<TokenStream2> = Vec::new();
    for a in el.attributes() {
        match a {
            NodeAttribute::Block(_) => {
                return Err(syn::Error::new_spanned(
                    a,
                    "view!: spread attributes are not supported; name each prop",
                ));
            }
            NodeAttribute::Attribute(attr) => {
                let key = node_name_string(&attr.key);
                let val = attr_value_tokens(&attr.clone());
                if key == "content" {
                    content_attr = Some(val);
                } else {
                    let method = format_ident!("{}", attr_method(&key));
                    modifiers.push(quote! { .#method(#val) });
                }
            }
        }
    }
    let child_content: Option<TokenStream2> = match el.children.as_slice() {
        [] => None,
        [single] => match single {
            Node::Text(t) => {
                let s = t.value_string();
                Some(quote! { #s })
            }
            Node::Block(b) => Some({
                let inner = match b {
                    rstml::node::NodeBlock::ValidBlock(block) => strip_braces(block),
                    other => quote! { #other },
                };
                quote! { #inner }
            }),
            Node::Element(_) | Node::Fragment(_) => {
                return Err(syn::Error::new_spanned(
                    single,
                    "view!: <Text> takes a single string/signal child, not nested elements — use sibling <Text> nodes",
                ));
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "view!: unsupported <Text> child",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &el.children[1],
                "view!: <Text> takes a single child; concatenate with `format!` or use sibling <Text> nodes",
            ));
        }
    };
    let content = match (content_attr, child_content) {
        (Some(_), Some(_)) => {
            return Err(syn::Error::new_spanned(
                el.name(),
                "view!: <Text> takes `content=` or a child, not both",
            ));
        }
        (Some(c), None) => c,
        (None, Some(c)) => c,
        (None, None) => {
            return Err(syn::Error::new_spanned(
                el.name(),
                "view!: <Text> needs a child or `content=`",
            ));
        }
    };
    base = quote! { #base(#content) };
    for m in modifiers {
        base = quote! { #base #m };
    }
    Ok(base)
}

pub fn expand(tokens: TokenStream2) -> syn::Result<TokenStream2> {
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(tokens).split_vec();
    let mut out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        out = quote! { #out #t; };
    }
    match nodes.as_slice() {
        [] => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: expected at least one element, string, or `{expr}` block",
        )),
        [single] => {
            let e = child_expr(single)?;
            Ok(quote! { #out ::aurora::dsl::IntoElement::into_element(#e) })
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: multiple roots — wrap them in a <Group> (Aurora has no fragment node)",
        )),
    }
}
