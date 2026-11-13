// Responsibility: Facet-tagged surface property definitions (Geometry, Spacing, Position).

use crate::engine::registry::{Facet, RxEngine, SurfacePropDef};
use quote::quote;

pub fn register_facets(mut engine: RxEngine) -> RxEngine {
    let px = quote! { impl ::riode::dsl::prop::Px };
    let size = quote! { impl ::riode::reactive::prop::IntoProp<::riode::prelude::Size> };
    let f32_p = quote! { impl ::riode::reactive::prop::IntoProp<f32> };
    let i32_p = quote! { impl ::riode::reactive::prop::IntoProp<i32> };

    // Geometry Facet
    engine = engine
        .register_surface_prop(SurfacePropDef {
            name: "w",
            facet: Facet::Geometry,
            arg_type: size.clone(),
            apply: quote! { self.surface.width = Some(w.into_prop()); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "width",
            facet: Facet::Geometry,
            arg_type: size.clone(),
            apply: quote! { self.surface.width = Some(width.into_prop()); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "h",
            facet: Facet::Geometry,
            arg_type: size.clone(),
            apply: quote! { self.surface.height = Some(h.into_prop()); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "height",
            facet: Facet::Geometry,
            arg_type: size.clone(),
            apply: quote! { self.surface.height = Some(height.into_prop()); self },
        });

    // Spacing Facet (Margins)
    engine = engine
        .register_surface_prop(SurfacePropDef { name: "m", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::all(m.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "mx", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::x(mx.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "my", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::y(my.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "mt", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::top(mt.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "mr", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::right(mr.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "mb", facet: Facet::Spacing, arg_type: px.clone(), apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::bottom(mb.px())); self } })
        .register_surface_prop(SurfacePropDef { name: "ml", facet: Facet::Spacing, arg_type: px, apply: quote! { self.surface.merge_margin(::riode::prelude::Margin::left(ml.px())); self } });

    // Position Facet
    engine = engine
        .register_surface_prop(SurfacePropDef {
            name: "absolute",
            facet: Facet::Position,
            arg_type: quote! { impl ::std::convert::Into<bool> },
            apply: quote! { self.surface.is_absolute = absolute.into(); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "x",
            facet: Facet::Position,
            arg_type: f32_p.clone(),
            apply: quote! { self.surface.x = Some(x.into_prop()); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "y",
            facet: Facet::Position,
            arg_type: f32_p,
            apply: quote! { self.surface.y = Some(y.into_prop()); self },
        })
        .register_surface_prop(SurfacePropDef {
            name: "z",
            facet: Facet::Position,
            arg_type: i32_p,
            apply: quote! { self.surface.z_index = Some(z.into_prop()); self },
        });

    engine
}
