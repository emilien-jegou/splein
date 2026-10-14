// Single responsibility: 2D spatial primitives, boundary tests, and damage tracking models.

use smallvec::SmallVec;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct ResolvedRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ResolvedRect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    #[inline(always)]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    #[inline(always)]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    #[inline(always)]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    #[inline(always)]
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Whether this box is empty or degenerates into subpixel floating-point noise.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.width < 0.1 || self.height < 0.1
    }

    /// Whether any part of this box meets `other`, testing a collapsed box as a point at its origin.
    ///
    /// `intersects` is exclusive, so a box squeezed to zero area never intersects anything and would
    /// be culled from the display list even while its content still overflows visibly onto the screen.
    /// Visibility decisions use this instead; damage accumulation keeps using `intersects`.
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.width >= 1.0 && self.height >= 1.0 {
            return self.intersects(other);
        }
        Self {
            x: self.x,
            y: self.y,
            width: self.width.max(1.0),
            height: self.height.max(1.0),
        }
        .intersects(other)
    }

    #[inline(always)]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    #[inline(always)]
    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Whether `other` lies fully inside this box (edges inclusive).
    #[inline]
    pub fn contains_rect(&self, other: &Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    /// Computes the intersection of two rectangles. Returns ZERO if disjoint.
    #[inline]
    pub fn intersect(&self, other: &Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());

        if r > x && b > y {
            Self::new(x, y, r - x, b - y)
        } else {
            Self::ZERO
        }
    }

    /// Computes the minimal bounding union of two rectangles.
    #[inline]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_r = self.right().max(other.right());
        let max_b = self.bottom().max(other.bottom());

        Self::new(min_x, min_y, max_r - min_x, max_b - min_y)
    }

    /// Expands the rectangle symmetrically in all directions.
    #[inline]
    pub fn expand(&self, amount: f32) -> Self {
        if self.is_empty() {
            return Self::ZERO;
        }
        Self::new(
            self.x - amount,
            self.y - amount,
            self.width + 2.0 * amount,
            self.height + 2.0 * amount,
        )
    }
    /// Converts logical coordinates to physical device pixels with outward rounding to eliminate fractional seams.
    #[inline]
    pub fn to_physical_outward(&self, scale_factor: f32) -> Self {
        if self.is_empty() {
            return Self::ZERO;
        }

        let left = (self.x * scale_factor).floor();
        let top = (self.y * scale_factor).floor();
        let right = (self.right() * scale_factor).ceil();
        let bottom = (self.bottom() * scale_factor).ceil();

        Self::new(left, top, right - left, bottom - top)
    }

    /// Returns this box translated by `delta`.
    #[inline]
    pub fn shifted_by(&self, delta: Point) -> Self {
        Self::new(
            self.x + delta.x,
            self.y + delta.y,
            self.width,
            self.height,
        )
    }

    /// Subtracts `cut` from this box, returning the up-to-four disjoint rects
    /// that remain (fewer when edges align; empty when `cut` covers all).
    pub fn subtract(&self, cut: &Self) -> SmallVec<[ResolvedRect; 4]> {
        let mut out = SmallVec::new();
        let hit = self.intersect(cut);
        if hit.is_empty() {
            if !self.is_empty() {
                out.push(*self);
            }
            return out;
        }
        // Left / right bands span the cut's height; top / bottom span full width.
        let left = Self::new(self.x, hit.y, hit.x - self.x, hit.height);
        let right = Self::new(hit.right(), hit.y, self.right() - hit.right(), hit.height);
        let top = Self::new(self.x, self.y, self.width, hit.y - self.y);
        let bottom = Self::new(self.x, hit.bottom(), self.width, self.bottom() - hit.bottom());
        for r in [left, right, top, bottom] {
            if !r.is_empty() {
                out.push(r);
            }
        }
        out
    }
}

/// A retained-pixel move a backend may apply before repainting damage.
///
/// Produced when scroll content translates inside its viewport: instead of
/// re-rasterizing the whole viewport, a capable backend memmoves the covered
/// region by `delta` (rounded to device px) and repaints only the damage —
/// which the damage plan keeps to the newly-exposed strip. Backends that do
/// not implement the blit expand each `clip` into damage (see
/// [`DamageRegion::expanded_without_blit`]) and repaint it from the new
/// scene: slower, but correct with no capability negotiation.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct ScrollShift {
    /// The viewport box whose retained pixels moved, in screen space.
    pub clip: ResolvedRect,
    /// Content translation this frame (`new_origin − old_origin`), logical px.
    pub delta: Point,
}

/// Disjoint physical damage region capped at 4 non-overlapping rectangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamageRegion {
    rects: SmallVec<[ResolvedRect; 4]>,
    /// Retained-pixel moves recorded this frame (see [`ScrollShift`]).
    ///
    /// Deliberately NOT merged by accumulation: `damage_for_age` unions rects
    /// only, so a shift never leaks into another frame's region (a stale
    /// shift would move pixels the aged damage also covers). Backends read
    /// shifts from the current frame's region only.
    scroll_shifts: Vec<ScrollShift>,
}

impl DamageRegion {
    pub const MAX_RECTS: usize = 4;

    #[inline(always)]
    pub fn new() -> Self {
        Self {
            rects: SmallVec::new(),
            scroll_shifts: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    #[inline(always)]
    pub fn rects(&self) -> &[ResolvedRect] {
        &self.rects
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.rects.clear();
        self.scroll_shifts.clear();
    }

    /// Scroll shifts recorded this frame, in visit order (parents before children).
    #[inline(always)]
    pub fn scroll_shifts(&self) -> &[ScrollShift] {
        &self.scroll_shifts
    }

    /// Records a retained-pixel move. The damage plan pushes the exposed
    /// strip separately; shifts never stand in for damage.
    pub fn push_scroll_shift(&mut self, clip: ResolvedRect, delta: Point) {
        if clip.is_empty() || (delta.x == 0.0 && delta.y == 0.0) {
            return;
        }
        self.scroll_shifts.push(ScrollShift { clip, delta });
    }

    /// Fallback for backends without a scroll blit: the shift clips pushed as
    /// ordinary damage, shifts cleared. Rasterizing the result from the new
    /// scene is always correct — it just repaints the viewport instead of
    /// moving pixels.
    pub fn expanded_without_blit(&self) -> Self {
        let mut out = Self {
            rects: self.rects.clone(),
            scroll_shifts: Vec::new(),
        };
        for shift in &self.scroll_shifts {
            out.push(shift.clip);
        }
        out
    }

    /// Pushes a damaged rectangle, merging overlapping areas and applying greedy clustering if capacity is exceeded.
    pub fn push(&mut self, mut rect: ResolvedRect) {
        if rect.is_empty() {
            return;
        }

        // 1. Absorb all intersecting or touching rects into `rect`
        let mut i = 0;
        while i < self.rects.len() {
            if self.rects[i].intersects(&rect) {
                rect = self.rects[i].union(&rect);
                self.rects.swap_remove(i);
            } else {
                i += 1;
            }
        }

        self.rects.push(rect);

        // 2. If exceeding capacity, merge the single pair with the least added dead area
        if self.rects.len() > Self::MAX_RECTS {
            let mut best_pair = (0, 1);
            let mut min_added_area = f32::INFINITY;

            for a in 0..self.rects.len() {
                for b in (a + 1)..self.rects.len() {
                    let u = self.rects[a].union(&self.rects[b]);
                    let added_area = u.area() - self.rects[a].area() - self.rects[b].area();
                    if added_area < min_added_area {
                        min_added_area = added_area;
                        best_pair = (a, b);
                    }
                }
            }

            let (a, b) = best_pair;
            self.rects[a] = self.rects[a].union(&self.rects[b]);
            self.rects.swap_remove(b);
        }
    }

    /// Minimal bounding box enclosing all damage rectangles.
    #[inline]
    pub fn bounding_box(&self) -> ResolvedRect {
        self.rects
            .iter()
            .fold(ResolvedRect::ZERO, |acc, r| acc.union(r))
    }
}

/// Zero-clone circular ring buffer tracking the last 4 frames of damage regions.
#[derive(Clone, Debug, Default)]
pub struct DamageRing {
    history: [DamageRegion; 4],
    head: usize,
}

impl DamageRing {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances to the next frame slot and returns a mutable reference to record Frame N damage.
    pub fn advance(&mut self) -> &mut DamageRegion {
        self.head = (self.head + 1) % 4;
        self.history[self.head].clear();
        &mut self.history[self.head]
    }

    /// Accesses the damage region of the current frame (Frame N).
    #[inline(always)]
    pub fn current(&self) -> &DamageRegion {
        &self.history[self.head]
    }

    /// Accesses the damage region of the current frame mutably.
    #[inline(always)]
    pub fn current_mut(&mut self) -> &mut DamageRegion {
        &mut self.history[self.head]
    }

    /// Computes the composite damage union for the past `age` frames according to swapchain buffer age.
    /// Age 0 = full window repaint.
    /// Age N = union of the last N frames.
    pub fn damage_for_age(&self, age: u8, full_window: ResolvedRect) -> DamageRegion {
        if age == 0 || age > 4 {
            let mut full = DamageRegion::new();
            full.push(full_window);
            return full;
        }

        let mut composite = DamageRegion::new();
        for i in 0..age as usize {
            let idx = (self.head + 4 - i) % 4;
            for rect in self.history[idx].rects() {
                composite.push(*rect);
            }
        }
        composite
    }
}
