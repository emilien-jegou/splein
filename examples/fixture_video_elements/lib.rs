// Responsibility: A third-party leaf element — the `video` tag — defined with
// zero support from `riode` or `riode-macros`. This is the entire extension
// contract:
//
//   1. implement `Primitive` (`measure`, `paint`, `as_any`; everything else
//      defaults to a no-op mapped to an engine phase),
//   2. expose a `build()` entry point under a module named after the tag,
//   3. `use` it — `view!` resolves `<video>` by plain name lookup at the call
//      site, exactly like the built-in `text` / `img` / `svg`.
//
// The fixture plays one still frame; a real element would decode video. What
// is under test is the open leaf path, not the codec.

pub mod video {
    use riode::dsl::{Element, IntoElement, LeafBuilder, LeafDef, LeafProps};
    use riode::foundation::{Constraints, IntrinsicSize, ResolvedRect};
    use riode::prelude::Primitive;
    use riode::scene::PaintContext;
    use riode::scene::image::ImageSource;
    use riode::tree::kind::{LeafChange, MeasureCtx};
    use riode::tree::{DirtyFlags, NodeState, NodeStyle};

    /// The retained video leaf: paints its frame letterboxed (CSS `contain`).
    pub struct VideoFrame {
        frame: ImageSource,
    }

    impl Primitive for VideoFrame {
        fn measure(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &MeasureCtx) -> IntrinsicSize {
            let (iw, ih) = (self.frame.width as f32, self.frame.height as f32);
            if iw <= 0.0 || ih <= 0.0 {
                return IntrinsicSize { width: 0.0, height: 0.0 };
            }
            let ratio = iw / ih;
            let mut w = if ctx.constraints.max_width.is_finite() {
                ctx.constraints.max_width.min(iw)
            } else {
                iw
            };
            let mut h = w / ratio;
            if ctx.constraints.max_height.is_finite() && h > ctx.constraints.max_height {
                h = ctx.constraints.max_height.max(0.0);
                w = h * ratio;
            }
            IntrinsicSize { width: w, height: h }
        }

        fn paint(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &mut PaintContext, bounds: ResolvedRect) {
            let (iw, ih) = (self.frame.width as f32, self.frame.height as f32);
            if iw <= 0.0 || ih <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
                return;
            }
            let scale = (bounds.width / iw).min(bounds.height / ih);
            let (w, h) = (iw * scale, ih * scale);
            let x = bounds.x + (bounds.width - w) * 0.5;
            let y = bounds.y + (bounds.height - h) * 0.5;
            ctx.draw_image(ResolvedRect::new(x, y, w, h), self.frame.clone());
        }

        fn reconcile(&self, next: &dyn Primitive) -> Option<LeafChange> {
            next.as_any().downcast_ref::<VideoFrame>()?;
            Some(LeafChange {
                dirty: DirtyFlags::MEASURE | DirtyFlags::LAYOUT,
                ..LeafChange::default()
            })
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// The declarative builder for `<video>`: the same [`LeafProps`] block every
    /// leaf carries, plus the frame source.
    #[derive(Clone)]
    pub struct VideoElement {
        props: LeafProps,
        frame: Option<ImageSource>,
    }

    impl LeafBuilder for VideoElement {
        fn leaf(&mut self) -> &mut LeafProps {
            &mut self.props
        }
    }

    impl VideoElement {
        /// Sets the pixels to play. The source of `<video src=…>`.
        pub fn src(mut self, frame: ImageSource) -> Self {
            self.frame = Some(frame);
            self
        }

        /// Lowers the builder into the erased leaf shape every element shares —
        /// the same path the built-ins take. No engine involvement.
        pub fn into_leaf(self) -> LeafDef {
            let mut def = LeafDef::new(VideoFrame {
                frame: self.frame.unwrap_or_else(ImageSource::empty),
            });
            def.props = self.props;
            def
        }
    }

    impl IntoElement for VideoElement {
        fn into_element(self) -> Element {
            Element::Leaf(self.into_leaf())
        }
    }

    /// The tag namespace: `<video>` lowers to `video::build()` and fills itself
    /// through setters (`.src(...)`, plus every [`LeafBuilder`] property).
    pub fn build() -> VideoElement {
        VideoElement { props: LeafProps::default(), frame: None }
    }
}
