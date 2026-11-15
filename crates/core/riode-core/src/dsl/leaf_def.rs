// Single responsibility: The single erased leaf element builder shared by built-ins and third parties.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::dsl::leaf::{LeafBuilder, LeafProps};
use crate::tree::binding::DynamicBindings;
use crate::tree::Primitive;
use std::rc::Rc;

/// The erased leaf element: a primitive instance, its outer layout properties,
/// and the reactive bindings its content resolves through. `text`, `img`, `svg`
/// and a third party's `video` all lower to this one shape — the engine has no
/// other leaf path.
#[derive(Clone)]
pub struct LeafDef {
    pub props: LeafProps,
    pub primitive: Rc<dyn Primitive>,
    /// Reactive bindings the scheduler evaluates between frames (text content,
    /// sizes, …). Empty for static leaves.
    pub bindings: DynamicBindings,
    /// Whether the content reacts to signals: gates the measure subscription
    /// that drives per-frame content updates.
    pub dynamic: bool,
}

impl LeafDef {
    pub fn new<P: Primitive + 'static>(primitive: P) -> Self {
        Self {
            props: LeafProps::default(),
            primitive: Rc::new(primitive),
            bindings: DynamicBindings::default(),
            dynamic: false,
        }
    }

    pub fn from_rc(primitive: Rc<dyn Primitive>) -> Self {
        Self {
            props: LeafProps::default(),
            primitive,
            bindings: DynamicBindings::default(),
            dynamic: false,
        }
    }
}

impl LeafBuilder for LeafDef {
    fn leaf(&mut self) -> &mut LeafProps {
        &mut self.props
    }
}

impl IntoElement for LeafDef {
    fn into_element(self) -> Element {
        Element::Leaf(self)
    }
}

/// The default-styled text leaf a bare string lowers to — `impl IntoElement
/// for &str/String/Signal<String>/…` lives in core by the orphan rule, but its
/// builder (`text`) lives in the elements crate, so core constructs the leaf
/// directly. Same defaults as `text::build()`: 14px, white, wrap.
pub fn plain_text<P: crate::reactive::prop::IntoProp<String>>(content: P) -> Element {
    use crate::reactive::prop::Prop;
    use crate::text::leaf::TextLeaf;
    use crate::text::TextConfig;

    let content = content.into_prop();
    let dynamic = content.is_dynamic();
    let resolved = match &content {
        Prop::Static(s) => s.clone(),
        _ => String::new(),
    };
    Element::Leaf(LeafDef {
        props: LeafProps::default(),
        primitive: Rc::new(TextLeaf::new(TextConfig { content: resolved, ..TextConfig::default() })),
        bindings: DynamicBindings { text: content.dynamic_closure(), ..Default::default() },
        dynamic,
    })
}
