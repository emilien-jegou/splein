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

/// `on:click` → `on_click`; `ref` → `bind_ref`; `foo-bar` → `foo_bar`; else identity.
pub(crate) fn attr_method(key: &str) -> String {
    if key == "ref" {
        return "bind_ref".to_string();
    }
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
            KVAttributeValue::InvalidBraced(b) => nested_element_value(&b.body),
        },
        KeyedAttributeValue::None => {
            // Bare `<X flag>` means boolean true.
            quote! { true }
        }
    }
}

/// Attr values holding a single nested element: `head={<Strip …/>}`.
///
/// Braced RSX fails Rust parsing, so rstml recovers it as `InvalidBraced` and
/// the inner tokens re-enter the element pipeline here — recursively, so
/// nesting composes to any depth. Non-`<` bodies keep the old passthrough so
/// genuine Rust errors still surface natively. A bare control node here is
/// rejected with a pointer (it must be a direct child); control nodes deeper
/// inside a nested element resolve through local tables, so
/// `{<Group><for/></Group>}` works wherever a single element fits.
fn nested_element_value(body: &TokenStream2) -> TokenStream2 {
    let is_rsx = matches!(
        body.clone().into_iter().next(),
        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '<'
    );
    if !is_rsx {
        return quote! { {#body} };
    }
    let mut flow = FlowTables::default();
    let rewritten = flow.extract(body.clone());
    flow.warn();
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        out = quote! { #out #t; };
    }
    match nodes.as_slice() {
        [Node::Element(el)] => {
            let tag = node_name_string(el.name());
            if matches!(tag.as_str(), "for" | "if" | "match" | "else" | "case") {
                return quote! {
                    #out ::std::compile_error!(
                        "view!: control nodes (`<for>`, `<if>`, `<match>`) must be direct children — drop the surrounding `{…}`"
                    )
                };
            }
            match element_expr(el, &flow) {
                Ok(e) => {
                    let e = match flow.splice(e) {
                        Ok(spliced) => spliced,
                        Err(err) => err.to_compile_error(),
                    };
                    quote! { #out (#e) }
                }
                Err(err) => {
                    let compile = err.to_compile_error();
                    quote! { #out #compile }
                }
            }
        }
        _ => quote! {
            #out ::std::compile_error!(
                "view!: an attribute holds one nested element — `<Panel head={<Strip …/>}>`; anything else goes in `{…}` as Rust"
            )
        },
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

pub(crate) fn children_vec(children: &[ViewNode], flow: &FlowTables) -> syn::Result<TokenStream2> {
    // Component children stay elements: control nodes compose under Groups,
    // never as direct component children (components forward `Vec<Element>`).
    for c in children {
        if is_control_child(c) {
            return Err(syn::Error::new_spanned(
                c,
                "view!: control nodes (`<for>`, `<if>`, `<match>`) compose under `<Group>` children, not as direct component children — hoist the node above the component or wrap it in a `<Group>`",
            ));
        }
    }
    let mut items = Vec::new();
    for c in children {
        items.push(child_expr(c, flow)?);
    }
    Ok(quote! { ::std::vec![#(#items),*] })
}

/// Whether a child is a control-flow placeholder (`{__aurora_for_N}` etc.).
/// Placeholder idents only originate in the pre-pass, so a user `{…}` block
/// can only match by naming one literally — which then resolves the same way.
pub(crate) fn is_control_child(node: &ViewNode) -> bool {
    match node {
        Node::Block(b) => {
            let inner = match b {
                rstml::node::NodeBlock::ValidBlock(block) => block,
                _ => return false,
            };
            if inner.stmts.len() != 1 {
                return false;
            }
            match inner.stmts.first() {
                Some(syn::Stmt::Expr(syn::Expr::Path(p), None)) => p
                    .path
                    .get_ident()
                    .is_some_and(|id| crate::flow_util::is_control_placeholder(&id.to_string())),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Emits one child as a segment expression: control placeholders resolve to
/// their `Dynamic` segment, anything else wraps in `Static`. Callers collect
/// these into the `vec![…]` that `.child_segments()` takes.
pub(crate) fn child_segment(
    node: &ViewNode,
    flow: &FlowTables,
) -> syn::Result<TokenStream2> {
    if is_control_child(node) {
        // Resolve through the shared tables: the placeholder ident expands to
        // a `ChildSegment::Dynamic` expression in place.
        let ident_ts: TokenStream2 = match node {
            Node::Block(rstml::node::NodeBlock::ValidBlock(block)) => {
                quote! { #block }
            }
            _ => unreachable!("is_control_child was true"),
        };
        return flow.splice(ident_ts);
    }
    let e = child_expr(node, flow)?;
    let e = flow.splice(e)?;
    Ok(quote! { ::aurora::dsl::ChildSegment::Static(::std::vec![#e]) })
}

/// Emits already-parsed children as a `vec![…]` of segments, for `<Group>`
/// (whose `.child_segments()` takes them flat, with no carrier node).
pub(crate) fn children_segments_vec(
    children: &[ViewNode],
    flow: &FlowTables,
) -> syn::Result<TokenStream2> {
    let mut items = Vec::new();
    for c in children {
        items.push(child_segment(c, flow)?);
    }
    Ok(quote! { ::std::vec![#(#items),*] })
}

fn child_expr(node: &ViewNode, flow: &FlowTables) -> syn::Result<TokenStream2> {
    match node {
        Node::Element(el) => {
            let e = element_expr(el, flow)?;
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
            let kids = children_vec(&f.children, flow)?;
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

pub(crate) fn element_expr(el: &Element, flow: &FlowTables) -> syn::Result<TokenStream2> {
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
        // `Group` children merge as ordered segments: statics wrap in
        // `Static`, control nodes resolve to their `Dynamic` segments, and no
        // carrier node ever sits between parent and rows. Other primitives
        // take no children (native error if given any, as before).
        if tag == "Group" {
            let ctor = primitive_ctor(&tag).unwrap();
            let mut out = apply_attrs(ctor, el)?;
            if !el.children.is_empty() {
                let kids = children_segments_vec(&el.children, flow)?;
                out = quote! { #out.child_segments(#kids) };
            }
            return Ok(out);
        }
        let ctor = primitive_ctor(&tag).unwrap();
        let mut out = apply_attrs(ctor, el)?;
        if !el.children.is_empty() {
            let kids = children_vec(&el.children, flow)?;
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
        let kids = children_vec(&el.children, flow)?;
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
            Node::Block(b) => {
                if is_control_child(single) {
                    return Err(syn::Error::new_spanned(
                        single,
                        "view!: control nodes (`<for>`, `<if>`, `<match>`) compose under `<Group>` children — a `<Text>` holds one string/signal, never branches",
                    ));
                }
                Some({
                    let inner = match b {
                        rstml::node::NodeBlock::ValidBlock(block) => strip_braces(block),
                        other => quote! { #other },
                    };
                    quote! { #inner }
                })
            }
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
    expand_inner(tokens)
}

/// Parses child-list tokens with control-flow extraction, returning parser
/// diagnostics plus the parsed nodes. Group children and control bodies share
/// it; what differs is how each child is emitted (elements vs segments).
pub(crate) fn parse_child_nodes(
    tokens: TokenStream2,
    flow: &mut FlowTables,
) -> syn::Result<(TokenStream2, Vec<ViewNode>)> {
    let rewritten = flow.extract(tokens);
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
    let mut error_out = quote! {};
    for e in errors {
        let t = e.emit_as_expr_tokens();
        error_out = quote! { #error_out #t; };
    }
    Ok((error_out, nodes))
}

/// Emits a `Vec<ChildSegment>` expression (braced with any parser
/// diagnostics): statics wrap in `Static`, control nodes resolve to their
/// `Dynamic` segments. Group children and control bodies (arm/row vecs for
/// `children_of`-style closures) share it.
pub(crate) fn expand_children_vec(tokens: TokenStream2) -> syn::Result<TokenStream2> {
    let mut flow = FlowTables::default();
    let (errors, nodes) = parse_child_nodes(tokens, &mut flow)?;
    let mut items = Vec::new();
    for c in &nodes {
        items.push(child_segment(c, &flow)?);
    }
    Ok(quote! { { #errors ::std::vec![#(#items),*] } })
}

/// Side tables for every control-flow node extracted from one `view!` input.
/// Extraction order is irrelevant: bodies re-enter [`expand_children_vec`],
/// which re-runs all extracts, so cross-kind nesting falls out of recursion.
#[derive(Default)]
pub(crate) struct FlowTables {
    pub for_nodes: Vec<crate::for_node::ForNode>,
    pub if_nodes: Vec<crate::if_node::IfNode>,
    pub match_nodes: Vec<crate::match_node::MatchNode>,
    /// Stashed raw attr-nested RSX (`attr_node`), expanded at splice time.
    pub attr_raw: Vec<TokenStream2>,
}

impl FlowTables {
    pub fn extract(&mut self, tokens: TokenStream2) -> TokenStream2 {
        let t = crate::for_node::extract(tokens, &mut self.for_nodes);
        let t = crate::if_node::extract(t, &mut self.if_nodes);
        let t = crate::match_node::extract(t, &mut self.match_nodes);
        crate::attr_node::extract(t, &mut self.attr_raw)
    }

    pub fn splice(&self, body: TokenStream2) -> syn::Result<TokenStream2> {
        crate::flow_util::splice_with(body, &|kind, n, span| self.resolve(kind, n, span))
    }

    /// Splices with `for`/`if`/`match` resolved locally and `attr`
    /// references delegated to `outer` (used by eager attr expansion, whose
    /// nested references always point at smaller, already-stashed indices).
    pub fn splice_attr_ref(
        &self,
        body: TokenStream2,
        outer: &FlowTables,
    ) -> syn::Result<TokenStream2> {
        crate::flow_util::splice_with(body, &|kind, n, span| match kind {
            "attr" => outer
                .attr_raw
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<attr>` placeholder"))
                .and_then(|raw| crate::attr_node::expand_attr(raw, outer)),
            _ => self.resolve(kind, n, span),
        })
    }

    fn resolve(&self, kind: &str, n: usize, span: proc_macro2::Span) -> syn::Result<TokenStream2> {
        match kind {
            "for" => {
                let node = self.for_nodes.get(n).ok_or_else(|| {
                    syn::Error::new(span, "view!: stale `<for>` placeholder")
                })?;
                Ok(crate::for_node::expand(node))
            }
            "if" => {
                let node = self.if_nodes.get(n).ok_or_else(|| {
                    syn::Error::new(span, "view!: stale `<if>` placeholder")
                })?;
                crate::if_node::expand(node)
            }
            "match" => {
                let node = self.match_nodes.get(n).ok_or_else(|| {
                    syn::Error::new(span, "view!: stale `<match>` placeholder")
                })?;
                crate::match_node::expand(node)
            }
            "attr" => self
                .attr_raw
                .get(n)
                .ok_or_else(|| syn::Error::new(span, "view!: stale `<attr>` placeholder"))
                .and_then(|raw| crate::attr_node::expand_attr(raw, self)),
            _ => Err(syn::Error::new(span, "view!: stale control-flow placeholder")),
        }
    }

    /// `<for>` without `key=` falls back to index identity: warn once per site.
    pub fn warn(&self) {
        for _n in self
            .for_nodes
            .iter()
            .filter(|n| matches!(n.key, crate::for_node::ForKey::Default))
        {
            proc_macro::Diagnostic::spanned(
                proc_macro2::Span::call_site().unwrap(),
                proc_macro::Level::Warning,
                "<for> without `key=` uses row index as identity — rows reset state on reorder. Add `key={.id}` (unique per item) if this list reorders, filters, or inserts.",
            )
            .emit();
        }
    }
}

fn expand_inner(tokens: TokenStream2) -> syn::Result<TokenStream2> {
    let mut flow = FlowTables::default();
    let rewritten = flow.extract(tokens);
    flow.warn();
    let config = rstml::ParserConfig::default().recover_block(true);
    let parser = rstml::Parser::new(config);
    let (nodes, errors) = parser.parse_recoverable(rewritten).split_vec();
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
            // Control nodes are never layout nodes, so they can never be the
            // root: there is no element to return. Wrap in a `<Group>`.
            if is_control_child(single) {
                return Err(syn::Error::new_spanned(
                    single,
                    "view!: control nodes (`<for>`, `<if>`, `<match>`) are never layout nodes — wrap them in a `<Group>`",
                ));
            }
            let e = child_expr(single, &flow)?;
            let body = quote! { ::aurora::dsl::IntoElement::into_element(#e) };
            let body = flow.splice(body)?;
            Ok(quote! { #out #body })
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "view!: multiple roots — wrap them in a <Group> (Aurora has no fragment node)",
        )),
    }
}
