// Single responsibility: Component contracts, element conversion, and the ambient hook scope.

use aurora_macros::component;

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

impl IntoElement for Vec<Element> {
    fn into_element(self) -> Element {
        crate::dsl::group().children(self).into_element()
    }
}

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

impl IntoElement for crate::reactive::Derived<String> {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl<F: Fn() -> String + 'static> IntoElement for F {
    fn into_element(self) -> Element {
        crate::dsl::text(self).into_element()
    }
}

impl IntoElement for crate::reactive::DynamicProp<String> {
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

/// Splices any `IntoElement` child in `view! { {expr} }` position.
pub fn view_child(expr: impl IntoElement) -> Element {
    expr.into_element()
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
    crate::dsl::group()
        .provided(grant)
        .children(children)
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
                "aurora: component body panicked during mount ({}); mounting empty placeholder",
                panic_message(&payload)
            );
            crate::dsl::group().into_element()
        }
    }
}

/// Named variant of [`component_scope`]: the body runs in a scope keyed by a
/// stable component identity (by convention `module_path!()::Name`, emitted by
/// `#[component]`), so ordinal hooks survive remounts and rebuilds while call
/// order is unchanged. Sibling instances of one component disambiguate by
/// child index (see [`Cx::enter_named`]); distinct modules can't collide.
///
/// Same panic contract as [`component_scope`]: outside a scope it panics
/// loudly; a panicking body mounts an empty placeholder.
pub fn component_scope_named<T: IntoElement>(path: &'static str, body: impl FnOnce() -> T) -> Element {
    let cx = Cx::expect_current();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cx.enter_named(path, body))) {
        Ok(built) => built.into_element(),
        Err(payload) => {
            tracing::error!(
                "aurora: component body `{path}` panicked during mount ({}); mounting empty placeholder",
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
    //! Its expansion references absolute `::aurora::` paths. Those resolve in a dependent crate
    //! but not here, so the macro was effectively unusable for the crate's own components -- the
    //! one place most likely to reach for it. The `extern crate self as aurora` alias in the
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
        // Before the alias this file did not compile at all: `::aurora` had no meaning here.
        // The body still needs an ambient scope to run in, which `Cx::enter` provides.
        let runtime = std::rc::Rc::new(std::cell::RefCell::new(
            crate::reactive::ReactiveRuntime::new(),
        ));
        let element = crate::reactive::Cx::new(runtime).enter(|| counter(3));
        assert_eq!(element.key(), None);
    }
}
