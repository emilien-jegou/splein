// Responsibility: Acceptance fixture 2 view — the overridden `<img>` rendered by `view!`.
//
// `use crate::img;` is the explicit import that wins over the prelude glob.
// The built-in never enters resolution.

use crate::img;
use riode::prelude::*;
use riode_shader::Fill;

pub const HOT_TITLE: &str = "Fixture — <img> override (contain, not cover)";
pub const HOT_WIDTH: u32 = 560;
pub const HOT_HEIGHT: u32 = 340;
pub const HOT_BG: u32 = 0x101014;

#[component(name = "fixture_img_override::Page")]
pub fn Page() -> impl IntoElement {
    build_ui()
}

pub fn build_ui() -> impl IntoElement {
    view! {
        <g
            direction="vertical"
            width="fill"
            height="fill"
            material={Fill::solid(0x101014)}
            alignment="center"
            distribution="center"
            gap=12.0
        >
            <text size={15.0} color={Color::WHITE}>"overridden <img> — letterboxed (contain), not cropped (cover)"</text>
            <img
                src={include_image!("../images-demo/assets/pexels-valentin-ivantsov-2154772556-39599882.jpg", 640, 427)}
                w={400.0}
                h={200.0}
            />
        </g>
    }
}
