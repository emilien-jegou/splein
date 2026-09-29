// Trait contracts for user components and element conversion.

use crate::dsl::element::Element;

pub trait IntoElement {
    fn into_element(self) -> Element;
}

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
