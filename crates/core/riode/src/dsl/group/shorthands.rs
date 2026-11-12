// Single responsibility: Ergonomic shorthands for Tailwind spacing, physical alignments, direction, and flags.

use super::GroupElement;
use crate::foundation::*;
use crate::reactive::prop::{IntoProp, Prop};
use std::rc::Rc;

impl GroupElement {
    pub fn col(mut self) -> Self {
        self.layout.direction = Direction::Vertical;
        self
    }
    pub fn row(mut self) -> Self {
        self.layout.direction = Direction::Horizontal;
        self
    }
    pub fn w<P: IntoProp<Size>>(self, p: P) -> Self {
        self.width(p)
    }
    pub fn h<P: IntoProp<Size>>(self, p: P) -> Self {
        self.height(p)
    }

    pub fn align_h(mut self, a: impl Into<PhysAlign>) -> Self {
        self.align_h = Some(a.into());
        self
    }
    pub fn align_v(mut self, a: impl Into<PhysAlign>) -> Self {
        self.align_v = Some(a.into());
        self
    }
    pub fn align(self, a: impl Into<PhysAlign>) -> Self {
        let a: PhysAlign = a.into();
        self.align_h(a).align_v(a)
    }

    pub fn gap_x(mut self, px: impl crate::dsl::prop::Px) -> Self {
        self.gap_x = Some(px.px());
        self
    }
    pub fn gap_y(mut self, px: impl crate::dsl::prop::Px) -> Self {
        self.gap_y = Some(px.px());
        self
    }
    pub fn ratio(self, r: impl Into<AspectRatio>) -> Self {
        self.aspect_ratio(r)
    }
    pub fn z<P: IntoProp<i32>>(self, z: P) -> Self {
        self.z_index(z)
    }

    pub fn scroll_y(mut self) -> Self {
        self.scroll = Some(crate::dsl::scroll::ScrollAxis::Vertical);
        self
    }
    pub fn scroll_x(mut self) -> Self {
        self.scroll = Some(crate::dsl::scroll::ScrollAxis::Horizontal);
        self
    }
    pub fn scroll(mut self) -> Self {
        self.scroll = Some(crate::dsl::scroll::ScrollAxis::Both);
        self
    }
    pub fn bounce(mut self, on: bool) -> Self {
        self.scroll_bounce = Some(on);
        self
    }

    pub fn m(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::all(v.px()));
        self
    }
    pub fn mx(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::x(v.px()));
        self
    }
    pub fn my(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::y(v.px()));
        self
    }
    pub fn mt(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::top(v.px()));
        self
    }
    pub fn mr(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::right(v.px()));
        self
    }
    pub fn mb(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::bottom(v.px()));
        self
    }
    pub fn ml(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.margin = self.margin.merged(Margin::left(v.px()));
        self
    }

    pub fn p(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::all(v.px()));
        self
    }
    pub fn px(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::x(v.px()));
        self
    }
    pub fn py(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::y(v.px()));
        self
    }
    pub fn pt(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::top(v.px()));
        self
    }
    pub fn pr(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::right(v.px()));
        self
    }
    pub fn pb(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::bottom(v.px()));
        self
    }
    pub fn pl(mut self, v: impl crate::dsl::prop::Px) -> Self {
        self.padding = self.padding.merged(Padding::left(v.px()));
        self
    }

    pub fn show<P: IntoProp<bool>>(self, v: P) -> Self {
        self.visible(v)
    }
    pub fn hide<P: IntoProp<bool>>(mut self, v: P) -> Self {
        let prop = v.into_prop();
        self.presence = Some(match prop {
            Prop::Static(on) => Prop::Static(if on {
                LayoutPresence::Absent
            } else {
                LayoutPresence::Present
            }),
            Prop::Dynamic(f) => Prop::Dynamic(Rc::new(move || {
                if f() {
                    LayoutPresence::Absent
                } else {
                    LayoutPresence::Present
                }
            })),
            Prop::Context(m) => Prop::Context(m.map(|on| {
                if on {
                    LayoutPresence::Absent
                } else {
                    LayoutPresence::Present
                }
            })),
        });
        self
    }
    pub fn visible<P: IntoProp<bool>>(mut self, v: P) -> Self {
        let prop = v.into_prop();
        self.presence = Some(match prop {
            Prop::Static(on) => Prop::Static(if on {
                LayoutPresence::Present
            } else {
                LayoutPresence::Absent
            }),
            Prop::Dynamic(f) => Prop::Dynamic(Rc::new(move || {
                if f() {
                    LayoutPresence::Present
                } else {
                    LayoutPresence::Absent
                }
            })),
            Prop::Context(m) => Prop::Context(m.map(|on| {
                if on {
                    LayoutPresence::Present
                } else {
                    LayoutPresence::Absent
                }
            })),
        });
        self
    }

    pub fn resolved_layout(&self) -> LayoutConfig {
        let mut layout = self.layout;
        if self.align_h.is_some() || self.align_v.is_some() {
            match layout.direction {
                Direction::Horizontal => {
                    if let Some(h) = self.align_h {
                        layout.distribution = h.as_distribution();
                    }
                    if let Some(v) = self.align_v {
                        layout.alignment = v.as_alignment();
                    }
                }
                Direction::Vertical => {
                    if let Some(h) = self.align_h {
                        layout.alignment = h.as_alignment();
                    }
                    if let Some(v) = self.align_v {
                        layout.distribution = v.as_distribution();
                    }
                }
            }
        }
        if self.gap_x.is_some() || self.gap_y.is_some() {
            let (mut gx, mut gy) = match layout.direction {
                Direction::Horizontal => (layout.gap.main_px(), layout.gap.cross_px()),
                Direction::Vertical => (layout.gap.cross_px(), layout.gap.main_px()),
            };
            if let Some(x) = self.gap_x {
                gx = x;
            }
            if let Some(y) = self.gap_y {
                gy = y;
            }
            layout.gap = match layout.direction {
                Direction::Horizontal => Gap::Split {
                    main: gx,
                    cross: gy,
                },
                Direction::Vertical => Gap::Split {
                    main: gy,
                    cross: gx,
                },
            };
        }
        layout
    }
}
