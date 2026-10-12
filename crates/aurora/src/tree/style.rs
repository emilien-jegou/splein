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
