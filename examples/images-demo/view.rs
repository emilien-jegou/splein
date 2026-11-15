// Responsibility: Declarative UI view showcasing build-time image and SVG asset compilation.

use riode::dsl::GroupElement;
use riode::prelude::*;
use riode_shader::{Fill, Shape};

#[component(name = "images_demo::Page")]
pub fn Page() -> GroupElement {
    view! {
        <g
            direction="vertical"
            width="fill"
            height="fill"
            alignment="center"
            distribution="start"
        >
            // Scales responsively to fill the width while preserving aspect ratio:
            <img
                src={include_image!("pexels-valentin-ivantsov-2154772556-39599882.jpg", 5760, 3840)}
                absolute=true
                w="full"
            />

            // SVGs have their natural viewBox dimensions inferred automatically:
            <g
                direction="horizontal"
                z=1
                mt=24.0
                gap=8.0
                px=14.0
                py=10.0
                shape={Shape::squircle(16.0)}
                material={Fill::solid(0x18181BCC)}
                alignment="center"
                distribution="center"
            >
                <svg src={include_svg!("Hugeicons1stBracketCircle.svg", 24, 24)} />
                <svg src={include_svg!("Hugeicons3dRotate.svg", 24, 24)} />
                <svg src={include_svg!("HugeiconsAbacus.svg", 24, 24)} />
                <svg src={include_svg!("HugeiconsAiImage.svg", 24, 24)} />
                <svg src={include_svg!("HugeiconsAirplaneTakeOff02.svg", 24, 24)} />
            </g>
        </g>
    }
}
