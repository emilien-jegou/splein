// Single responsibility: Immutable styling intent and layout constraints for retained nodes.

use crate::foundation::{Anchor, Appearance, LayoutConfig, Margin, Padding, Size};
use crate::input::HitTest;

/// Declared styling specification and layout constraints on a tree node.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeStyle {
    pub width: Size,
    pub height: Size,
    pub layout: LayoutConfig,
    pub margin: Margin,
    pub padding: Padding,
    pub shrink: f32,
    pub is_absolute: bool,
    pub clip: bool,
    /// Marks a clipped box as a scroll viewport: compile keeps (rather than
    /// culls) the `overscroll` band around it so content scrolled into view
    /// was already encoded, and damage applies the scroll rule. No behavior
    /// change on its own — the offset lives on the content child binding.
    pub scrollable: bool,
    /// Preload band in logical px per edge, encoded (and rasterized) beyond
    /// the viewport so steady scrolling replays retained chunks instead of
    /// re-encoding entering rows. Zero means viewport-only.
    pub overscroll: f32,
    /// Compile each child of this node as its own scene chunk.
    ///
    /// Paint order is unchanged (same z, stable order), but clean children
    /// replay instead of re-encoding when a sibling changes — so a scroll
    /// tick replays every row and re-encodes only what moved. Set by
    /// `ScrollView` on its content wrapper; userland lists can opt in for
    /// the same row-granular replay.
    pub isolate_children: bool,
    pub anchor: Option<Anchor>,
    pub appearance: Appearance,
    pub z_index: i32,
    pub is_overlay: bool,
    pub has_layer: bool,
    /// Width-to-height ratio (`width / height`). When set and exactly one axis
    /// is determined (fixed/fill/tight/percent) while the other is `Fit`, the
    /// `Fit` axis derives from the determined one instead of content.
    pub aspect_ratio: Option<f32>,
    /// Hit-test participation; retained so hit testing mirrors paint order.
    pub hit_test: HitTest,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            width: Size::Fit,
            height: Size::Fit,
            layout: LayoutConfig::default(),
            margin: Margin::ZERO,
            padding: Padding::ZERO,
            shrink: 1.0, // W3C Flexbox default
            is_absolute: false,
            clip: false,
            scrollable: false,
            overscroll: 0.0,
            isolate_children: false,
            anchor: None,
            appearance: Appearance::default(),
            z_index: 0,
            is_overlay: false,
            has_layer: false,
            aspect_ratio: None,
            hit_test: HitTest::Auto,
        }
    }
}
