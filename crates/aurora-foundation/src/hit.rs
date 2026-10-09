// Single responsibility: Pointer hit-testing participation policy.

/// How a node participates in pointer hit testing.
///
/// `Auto` is the default: handler-less nodes are skipped by the router's target
/// search, so decorative wrappers never swallow input meant for the control inside.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HitTest {
    /// Default participation: skipped by target search when handler-less.
    #[default]
    Auto,
    /// Solid even without handlers: absorbs hits so clicks on dead space never
    /// ghost through to nodes beneath (menus, modals). Acts as an implicit stop.
    Opaque,
    /// Invisible to collision: the subtree is skipped as if it were not there.
    Passthrough,
}
