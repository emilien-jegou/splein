// Responsibility: Acceptance fixture 1 — a third-party `video` element rendered by `view!`.
//
// The element comes from an external crate, imported explicitly. Nothing in
// `riode` or `riode-macros` knows the tag: the generic lowercase lowering emits
// `video::build()` and the call-site import resolves it.

use fixture_video_elements::video;
use riode::prelude::*;
use riode_shader::Fill;

pub const HOT_TITLE: &str = "Fixture — third-party <video>";
pub const HOT_WIDTH: u32 = 560;
pub const HOT_HEIGHT: u32 = 340;
pub const HOT_BG: u32 = 0x101014;

#[component(name = "fixture_video::Page")]
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
            <text size={15.0} color={Color::WHITE}>"third-party <video> tag — letterboxed by its own Primitive"</text>
            <video
                src={include_image!("../images-demo/assets/pexels-valentin-ivantsov-2154772556-39599882.jpg", 640, 427)}
                w={400.0}
                h={225.0}
            />
        </g>
    }
}
