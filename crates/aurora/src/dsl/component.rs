// Single responsibility: Component contracts, element conversion, and the ambient hook scope.

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

pub struct ComponentElement<C: Component>(pub C);

impl<C: Component> IntoElement for ComponentElement<C> {
    fn into_element(self) -> Element {
        self.0.render()
    }
}

pub fn component<C: Component>(comp: C) -> Element {
    comp.render()
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
pub fn component_scope<T: IntoElement>(body: impl FnOnce() -> T) -> Element {
    let cx = Cx::expect_current();
    cx.enter(body).into_element()
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
    use aurora_foundation::Color;
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
