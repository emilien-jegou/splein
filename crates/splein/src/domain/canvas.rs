use super::geometry::{
    point_to_segment_distance, Aabb, CubicBezierSegment, InputSample, MidpointSpline, Vec2,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub color: Rgba,
    pub segments: Vec<CubicBezierSegment>,
    pub live_tip: Option<(Vec2, Vec2)>,
    pub base_width: f32,
    pub aabb: Aabb,
    pub pending_segment: Option<CubicBezierSegment>,
    spline: MidpointSpline,
}

impl Stroke {
    pub fn new(color: Rgba, base_width: f32) -> Self {
        Self {
            color,
            segments: Vec::with_capacity(64),
            live_tip: None,
            base_width,
            aabb: Aabb::EMPTY,
            pending_segment: None,
            spline: MidpointSpline::new(),
        }
    }

    pub fn push_sample(&mut self, sample: InputSample) {
        self.aabb.expand_with_point(sample.position);
        if let Some(segment) = self.spline.push(sample) {
            self.segments.push(segment);
            self.pending_segment = Some(segment);
        }
        self.live_tip = self.spline.live_tip();
    }

    pub fn finalize(&mut self) -> Vec<CubicBezierSegment> {
        let remaining = self.spline.finish();
        self.segments.extend_from_slice(&remaining);
        self.live_tip = None;
        remaining
    }

    #[inline]
    pub fn intersects(&self, pos: Vec2, radius: f32) -> bool {
        if !self.aabb.contains_with_padding(pos, radius) {
            return false;
        }

        let samples = self.spline.samples();
        if samples.len() < 2 {
            return false;
        }
        for window in samples.windows(2) {
            if point_to_segment_distance(pos, window[0].position, window[1].position) <= radius {
                return true;
            }
        }
        false
    }

    pub fn translate(&mut self, delta: Vec2) {
        self.aabb.translate(delta);
        for seg in &mut self.segments {
            seg.start.x += delta.x;
            seg.start.y += delta.y;
            seg.ctrl1.x += delta.x;
            seg.ctrl1.y += delta.y;
            seg.ctrl2.x += delta.x;
            seg.ctrl2.y += delta.y;
            seg.end.x += delta.x;
            seg.end.y += delta.y;
        }
        if let Some((ref mut from, ref mut to)) = self.live_tip {
            from.x += delta.x;
            from.y += delta.y;
            to.x += delta.x;
            to.y += delta.y;
        }
    }
}

#[derive(Debug, Clone)]
pub enum DrawingElement {
    Stroke(Stroke),
    Highlighter(Stroke),
    Line { start: Vec2, end: Vec2, color: Rgba, width: f32, aabb: Aabb },
    Rect { min: Vec2, max: Vec2, color: Rgba, width: f32, aabb: Aabb },
    Ellipse { center: Vec2, rx: f32, ry: f32, color: Rgba, width: f32, aabb: Aabb },
}

impl DrawingElement {
    pub fn aabb(&self) -> Aabb {
        match self {
            Self::Stroke(s) | Self::Highlighter(s) => s.aabb,
            Self::Line { aabb, .. } | Self::Rect { aabb, .. } | Self::Ellipse { aabb, .. } => *aabb,
        }
    }

    #[inline]
    pub fn intersects(&self, pos: Vec2, radius: f32) -> bool {
        if !self.aabb().contains_with_padding(pos, radius) {
            return false;
        }

        match self {
            Self::Stroke(s) | Self::Highlighter(s) => s.intersects(pos, radius),
            Self::Line { start, end, .. } => point_to_segment_distance(pos, *start, *end) <= radius,
            Self::Rect { min, max, .. } => {
                let p1 = *min;
                let p2 = Vec2::new(max.x, min.y);
                let p3 = *max;
                let p4 = Vec2::new(min.x, max.y);
                point_to_segment_distance(pos, p1, p2) <= radius
                    || point_to_segment_distance(pos, p2, p3) <= radius
                    || point_to_segment_distance(pos, p3, p4) <= radius
                    || point_to_segment_distance(pos, p4, p1) <= radius
            }
            Self::Ellipse { center, rx, ry, .. } => {
                if *rx <= 0.0 || *ry <= 0.0 {
                    return false;
                }
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let normalized_dist = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
                let approx_dist = (normalized_dist - 1.0).abs() * ((rx + ry) / 2.0);
                approx_dist <= radius
            }
        }
    }

    pub fn translate(&mut self, delta: Vec2) {
        match self {
            Self::Stroke(s) | Self::Highlighter(s) => s.translate(delta),
            Self::Line { start, end, aabb, .. } => {
                start.x += delta.x;
                start.y += delta.y;
                end.x += delta.x;
                end.y += delta.y;
                aabb.translate(delta);
            }
            Self::Rect { min, max, aabb, .. } => {
                min.x += delta.x;
                min.y += delta.y;
                max.x += delta.x;
                max.y += delta.y;
                aabb.translate(delta);
            }
            Self::Ellipse { center, aabb, .. } => {
                center.x += delta.x;
                center.y += delta.y;
                aabb.translate(delta);
            }
        }
    }
}

#[derive(Default)]
pub struct Canvas {
    elements: Vec<DrawingElement>,
    active_element: Option<DrawingElement>,
    pub needs_full_rebuild: bool,
}

impl Canvas {
    pub fn begin_freehand(&mut self, color: Rgba, base_width: f32, is_highlighter: bool, initial: InputSample) {
        let mut stroke = Stroke::new(color, base_width);
        stroke.push_sample(initial);
        if is_highlighter {
            self.active_element = Some(DrawingElement::Highlighter(stroke));
        } else {
            self.active_element = Some(DrawingElement::Stroke(stroke));
        }
    }

    pub fn extend_freehand(&mut self, sample: InputSample) {
        match self.active_element.as_mut() {
            Some(DrawingElement::Stroke(s)) | Some(DrawingElement::Highlighter(s)) => {
                s.push_sample(sample);
            }
            _ => {}
        }
    }

    pub fn take_pending_segment(&mut self) -> Option<(CubicBezierSegment, Rgba, f32, bool)> {
        match self.active_element.as_mut() {
            Some(DrawingElement::Stroke(s)) => {
                s.pending_segment.take().map(|seg| (seg, s.color, s.base_width, false))
            }
            Some(DrawingElement::Highlighter(s)) => {
                s.pending_segment.take().map(|seg| (seg, s.color, s.base_width, true))
            }
            _ => None,
        }
    }

    pub fn update_shape_preview(&mut self, elem: DrawingElement) {
        self.active_element = Some(elem);
    }

    pub fn commit_active(&mut self) -> Vec<(CubicBezierSegment, Rgba, f32, bool)> {
        let mut final_segments = Vec::new();
        if let Some(mut elem) = self.active_element.take() {
            match &mut elem {
                DrawingElement::Stroke(s) => {
                    let segs = s.finalize();
                    for seg in segs {
                        final_segments.push((seg, s.color, s.base_width, false));
                    }
                }
                DrawingElement::Highlighter(s) => {
                    let segs = s.finalize();
                    for seg in segs {
                        final_segments.push((seg, s.color, s.base_width, true));
                    }
                }
                _ => {
                    self.needs_full_rebuild = true;
                }
            }
            self.elements.push(elem);
        }
        final_segments
    }

    pub fn erase_at(&mut self, pos: Vec2, radius: f32) -> bool {
        let initial_len = self.elements.len();
        self.elements.retain(|elem| !elem.intersects(pos, radius));
        let changed = self.elements.len() != initial_len;
        if changed {
            self.needs_full_rebuild = true;
        }
        changed
    }

    pub fn hit_test(&self, pos: Vec2, padding: f32) -> Option<usize> {
        for (i, elem) in self.elements.iter().enumerate().rev() {
            if elem.intersects(pos, padding) {
                return Some(i);
            }
        }
        None
    }

    pub fn translate_element(&mut self, idx: usize, delta: Vec2) {
        if let Some(elem) = self.elements.get_mut(idx) {
            elem.translate(delta);
            self.needs_full_rebuild = true;
        }
    }

    pub fn undo(&mut self) -> bool {
        let ok = self.elements.pop().is_some();
        if ok {
            self.needs_full_rebuild = true;
        }
        ok
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.active_element = None;
        self.needs_full_rebuild = true;
    }

    pub fn elements(&self) -> &[DrawingElement] {
        &self.elements
    }

    pub fn active_element(&self) -> Option<&DrawingElement> {
        self.active_element.as_ref()
    }
}
