// Responsibility: Declarative registration of Riode UI core primitives and their supported surface facets.

pub mod attr;
pub mod facets;
pub mod group;
pub mod media;
pub mod text;

use crate::engine::registry::{Facet, PrimitiveDef, RxEngine};

/// Builds the Riode engine by registering concrete UI primitives with their physical facets.
pub fn riode_engine() -> RxEngine {
    let engine = RxEngine::new()
        .register_primitive(PrimitiveDef {
            tag: "g",
            aliases: &["Group"],
            type_names: &["GroupElement"],
            expand: group::expand,
            supported_facets: vec![Facet::Geometry, Facet::Spacing, Facet::Position, Facet::Visibility],
        })
        .register_primitive(PrimitiveDef {
            tag: "text",
            aliases: &["Text"],
            type_names: &["TextElement"],
            expand: text::expand,
            supported_facets: vec![Facet::Geometry, Facet::Position, Facet::Visibility],
        })
        .register_primitive(PrimitiveDef {
            tag: "img",
            aliases: &["Image"],
            type_names: &["ImageElement"],
            expand: media::expand_img,
            supported_facets: vec![Facet::Geometry, Facet::Position, Facet::Visibility],
        })
        .register_primitive(PrimitiveDef {
            tag: "svg",
            aliases: &["Svg"],
            type_names: &["SvgElement"],
            expand: media::expand_svg,
            supported_facets: vec![Facet::Geometry, Facet::Position, Facet::Visibility],
        });

    facets::register_facets(engine)
}
