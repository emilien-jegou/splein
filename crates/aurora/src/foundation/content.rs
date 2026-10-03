// Single responsibility: How a node's interior reacts while its render box changes size.

/// Who owns a node's render box while it is away from its flow box.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RenderDriver {
    /// The render box springs toward the flow box.
    #[default]
    Auto,
    /// A pointer or gesture writes the render box directly, ignoring the flow box.
    ///
    /// This is what makes drag simple: the held node stops following layout, siblings
    /// keep springing to their own flow boxes, and release hands the pointer's velocity
    /// to the spring. No dragged flag, no stride compensation.
    Pointer,
}
