// Single responsibility: Component contracts, element conversion, and the ambient hook scope.

use riode_macros::component;

use crate::dsl::element::Element;
use crate::reactive::Cx;

/// Converts a built value into a mountable element.
pub trait IntoElement {
    fn into_element(self) -> Element;
}

/// A value that renders itself, for components that carry state rather than arguments.
pub trait Component {
    fn render(self) -> Element;
}

impl IntoElement for Element {
    fn into_element(self) -> Element {
        self
    }
}

// Note: there is deliberately no `IntoElement for Vec<Element>`. A vec in
// element position used to wrap itself in an anonymous group, which silently
// corrupted layout wherever `{children}`/`{rows}` was spliced (F1). Vectors
// splice flat through `IntoSplice`; a body that wants a box says so with an
// explicit `group().children(…)`.

impl IntoElement for String {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl IntoElement for &'static str {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl IntoElement for crate::reactive::Signal<String> {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl IntoElement for crate::reactive::Computed<String> {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl<F: Fn() -> String + 'static> IntoElement for F {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl IntoElement for crate::reactive::Prop<String> {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

macro_rules! impl_display_element {
    ($($t:ty),*) => {
        $(
            impl IntoElement for $t {
                fn into_element(self) -> Element {
                    crate::dsl::text(self.to_string()).into_element()
                }
            }
        )*
    };
}

impl_display_element!(f32, i32, u32, usize, isize, bool);

/// Splices one `view! { {expr} }` child into its parent's flat child list.
///
/// The distinguishing case is [`Vec<Element>`]: it contributes its elements as
/// siblings — no wrapper node — so `<Panel>{children}</Panel>` and a `{rows}`
/// splice forward flat by construction. Everything else contributes exactly
/// one element, via its [`IntoElement`] impl.
///
/// The blanket impl is only coherent because `Vec<Element>` deliberately no
/// longer implements `IntoElement` (see the note above); re-adding that impl
/// would reintroduce the F1 wrapper bug *and* fail to compile here.
pub trait IntoSplice {
    fn splice_into(self, out: &mut Vec<Element>);
}

impl<T: IntoElement> IntoSplice for T {
    fn splice_into(self, out: &mut Vec<Element>) {
        out.push(self.into_element());
    }
}

impl IntoSplice for Vec<Element> {
    fn splice_into(self, out: &mut Vec<Element>) {
        out.extend(self);
    }
}

/// Splices any [`IntoSplice`] child in `view! { {expr} }` position: a
/// `Vec<Element>` contributes its elements as siblings (no wrapper node);
/// anything else contributes one element.
pub fn view_child(expr: impl IntoSplice) -> Vec<Element> {
    let mut out = Vec::new();
    expr.splice_into(&mut out);
    out
}

pub struct ComponentElement<C: Component>(pub C);

impl<C: Component> IntoElement for ComponentElement<C> {
    fn into_element(self) -> Element {
        self.0.render()
    }
}

pub fn component<C: Component>(comp: C) -> Element {
    comp.render()
}

/// Establishes a context value for its subtree (see `reactive::context`).
///
/// The grant pairs handle and signal with statically enforced types, so a
/// `Signal<Theme>` can never be provided under a `Context<Border>`. Children
/// forward flat through a passthrough group — no wrapper node of its own.
///
/// ```ignore
/// view! {
///     <Provider grant={Theme.provide(theme_sig)}>
///         <Card />
///     </Provider>
/// }
/// ```
#[component]
pub fn Provider(
    grant: crate::reactive::context::Provided,
    children: Vec<Element>,
) -> impl IntoElement {
    crate::dsl::group().provided(grant).children(children)
}

/// Runs a component body inside the ambient hook scope, converting the result to an element.
///
/// This is what `#[component]` expands to. The body executes exactly once per mount: the
/// engine never re-runs a component to animate it, because a signal captured in a property
/// binding keeps that node reading the live value every frame. So a component's reactive
/// state lives in the signals its body allocated, not in a re-render cycle.
///
/// Panics outside a scope rather than quietly inventing a runtime, because a component
/// built against a throwaway graph would drive nothing while appearing to work.
///
/// A panicking *body* does not propagate: it logs loudly and mounts a zero-size
/// placeholder, so one bad component can't abort the reconcile walk (or, in a
/// hot-reload loop, kill the app). Signals the body allocated before panicking
/// are not reclaimed — bounded leak per failed mount, acceptable for a dev loop.
pub fn component_scope<T: IntoElement>(body: impl FnOnce() -> T) -> Element {
    let cx = Cx::expect_current();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cx.enter(body))) {
        Ok(built) => built.into_element(),
        Err(payload) => {
            tracing::error!(
                "riode: component body panicked during mount ({}); mounting empty placeholder",
                panic_message(&payload)
            );
            crate::dsl::group().into_element()
        }
    }
}

/// Named variant of [`component_scope`]: the body runs in a scope keyed by a
/// stable component identity (by convention `file!()::Name`, emitted by
/// `#[component]`), so ordinal hooks survive remounts and rebuilds while call
/// order is unchanged. Sibling instances of one component disambiguate by
/// child index (see [`Cx::enter_named`]); distinct modules can't collide.
///
/// `source_file` is the `file!()` at the expansion site (passed by the macro, not by
/// hand): the hot-reload registry files it so a file change can name the components it
/// must rebuild. Every build — mount or rebuild — files a note; the engine drains them
/// once per frame.
///
/// The note files from *inside* the entered scope so it carries the full scope identity
/// (parent chain plus child index), not just the component path: rebuild sweeps match on
/// instances to drop superseded registrations without touching live ones.
///
/// Same panic contract as [`component_scope`]: outside a scope it panics
/// loudly; a panicking body mounts an empty placeholder.
pub fn component_scope_named<T: IntoElement>(
    path: &'static str,
    source_file: &'static str,
    body: impl FnOnce() -> T,
) -> Element {
    let cx = Cx::expect_current();
    let scoped = || {
        // Filed first inside the scope: even a panicking body leaves its note, and the
        // instance is the entered one (ancestors included), never the parent's.
        if let Some(scope) = crate::reactive::hooks::current_scope() {
            let instance: Box<str> = Box::<str>::from(&*scope.instance);
            crate::hot_reload::registry::note_build(instance, path, source_file);
        }
        body()
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cx.enter_named(path, scoped)
    })) {
        Ok(built) => built.into_element(),
        Err(payload) => {
            tracing::error!(
                "riode: component body `{path}` panicked during mount ({}); mounting empty placeholder",
                panic_message(&payload)
            );
            crate::dsl::group().into_element()
        }
    }
}

/// Best-effort panic payload rendering for error logs.
pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    //! `#[component]` has to work from inside the crate, not only from a downstream one.
    //!
    //! Its expansion references absolute `::riode::` paths. Those resolve in a dependent crate
    //! but not here, so the macro was effectively unusable for the crate's own components -- the
    //! one place most likely to reach for it. The `extern crate self as riode` alias in the
    //! crate root is what makes one expansion valid on both sides. The failure mode this guards
    //! is a compile error, so the test has to be compiled rather than merely run.

    use crate::dsl::component::IntoElement;
    use crate::dsl::element::Element;
    use crate::dsl::group;
    use crate::foundation::Color;
    use crate::prelude::component;

    /// A component body returns something `IntoElement`; the macro's wrapper converts it.
    #[component]
    fn counter(taps: u32) -> Element {
        group()
            .width(10.0 + taps as f32)
            .height(10.0)
            .fill(Color::WHITE)
            .into_element()
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn component_macro_resolves_inside_the_crate() {
        // Before the alias this file did not compile at all: `::riode` had no meaning here.
        // The body still needs an ambient scope to run in, which `Cx::enter` provides.
        let runtime = std::rc::Rc::new(std::cell::RefCell::new(
            crate::reactive::ReactiveRuntime::new(),
        ));
        let element = crate::reactive::Cx::new(runtime).enter(|| counter(3));
        assert_eq!(element.key(), None);
    }
}
