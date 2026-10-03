// Single responsibility: A cheap, copyable snapshot of a rasterized frame for pixel assertions.

use tiny_skia::{Pixmap, PremultipliedColorU8};

/// An 8-bit RGBA colour, comparable and printable in a failure message.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<PremultipliedColorU8> for Rgba {
    fn from(p: PremultipliedColorU8) -> Self {
        Self {
            r: p.red(),
            g: p.green(),
            b: p.blue(),
            a: p.alpha(),
        }
    }
}

impl Rgba {
    /// Whether this colour is within `tolerance` of `other` on every channel.
    #[inline]
    pub fn near(self, other: Rgba, tolerance: u8) -> bool {
        self.r.abs_diff(other.r) <= tolerance
            && self.g.abs_diff(other.g) <= tolerance
            && self.b.abs_diff(other.b) <= tolerance
            && self.a.abs_diff(other.a) <= tolerance
    }
}

/// One rasterized frame, held by value.
///
/// `Engine::canvas` re-rasterizes on every call, so a per-pixel scan that calls it per probe
/// re-renders the frame each time. Taking one snapshot and querying *that* makes pixel work
/// cost one copy.
#[derive(Clone)]
pub struct Snapshot {
    pixels: Pixmap,
}

impl Snapshot {
    /// Copies the engine's current canvas.
    pub fn capture(pixels: &Pixmap) -> Self {
        Self {
            pixels: pixels.clone(),
        }
    }

    /// Width in pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.pixels.width()
    }

    /// Height in pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.pixels.height()
    }

    /// The colour at a pixel, or `None` when out of bounds.
    #[inline]
    pub fn at(&self, x: u32, y: u32) -> Option<Rgba> {
        self.pixels.pixel(x, y).map(Rgba::from)
    }

    /// The colour at a pixel, panicking when out of bounds.
    ///
    /// Bounds mistakes are the single most common source of silently wrong pixel assertions,
    /// so a probe that runs off the edge should fail loudly rather than read transparent.
    #[inline]
    pub fn color(&self, x: u32, y: u32) -> Rgba {
        self.at(x, y).unwrap_or_else(|| {
            panic!(
                "({x},{y}) is outside the {}x{} frame",
                self.width(),
                self.height()
            )
        })
    }

    /// Whether the frame contains no pixel matching `predicate`.
    pub fn none_match(&self, predicate: impl Fn(Rgba) -> bool) -> bool {
        self.pixels
            .pixels()
            .iter()
            .all(|p| !predicate(Rgba::from(*p)))
    }

    /// How many pixels match `predicate`, sampled on a grid of the given step.
    ///
    /// Sampling exists because a full-frame scan of a 800x760 canvas is 600k probes, which
    /// is fine once but ruinous inside a per-frame loop.
    pub fn count_matching(&self, step: u32, predicate: impl Fn(Rgba) -> bool) -> usize {
        let step = step.max(1);
        let mut n = 0;
        for y in (0..self.height()).step_by(step as usize) {
            for x in (0..self.width()).step_by(step as usize) {
                if let Some(p) = self.pixels.pixel(x, y) {
                    if predicate(Rgba::from(p)) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Contiguous runs of matching pixels down a column, as inclusive `(start, end)` rows.
    ///
    /// A single column is enough to see a stack of cards or a list: each card reads as one
    /// solid run, and the gap between runs is the separation that motion is judged on.
    pub fn runs_down(&self, x: u32, predicate: impl Fn(Rgba) -> bool) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let mut start: Option<u32> = None;
        for y in 0..self.height() {
            let hit = self.at(x, y).is_some_and(&predicate);
            match (hit, start) {
                (true, None) => start = Some(y),
                (false, Some(s)) => {
                    runs.push((s, y - 1));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            runs.push((s, self.height() - 1));
        }
        runs
    }

    /// The widest run down a column, and the row just past its end.
    ///
    /// Answers "how tall did this card actually paint" without needing its layout rect,
    /// which is the whole question when a box is animating.
    pub fn widest_run(&self, x: u32, predicate: impl Fn(Rgba) -> bool) -> Option<(u32, u32)> {
        self.runs_down(x, predicate)
            .into_iter()
            .max_by_key(|(a, b)| b - a)
    }

    /// Pixels that differ between two frames beyond `tolerance`, as a count.
    ///
    /// A transition should change the frame on every step while it runs and stop changing
    /// once it settles, so this doubles as "is anything still moving".
    pub fn changed_pixels(&self, other: &Snapshot, tolerance: u8) -> usize {
        if (self.width(), self.height()) != (other.width(), other.height()) {
            return self.width() as usize * self.height() as usize;
        }
        self.pixels
            .pixels()
            .iter()
            .zip(other.pixels.pixels())
            .filter(|(a, b)| !Rgba::from(**a).near(Rgba::from(**b), tolerance))
            .count()
    }

    /// Whether two frames are identical within `tolerance`.
    pub fn matches(&self, other: &Snapshot, tolerance: u8) -> bool {
        self.changed_pixels(other, tolerance) == 0
    }

    /// Flattens this frame onto an opaque background.
    ///
    /// The CPU canvas leaves anything unpainted transparent, because clearing is the
    /// backend's job and only the windowed backends know the page colour. Compositing here
    /// is what makes a headless capture look like the running app rather than a dark void,
    /// and what makes a pixel assertion about "the page showing through" mean anything.
    pub fn over(&self, background: Rgba) -> Snapshot {
        let mut out = Pixmap::new(self.width().max(1), self.height().max(1)).expect("alloc");
        for (dst, src) in out.pixels_mut().iter_mut().zip(self.pixels.pixels().iter()) {
            let s = Rgba::from(*src);
            let a = s.a as u32;
            let mix = |fg: u8, bg: u8| -> u8 {
                ((fg as u32 * a + bg as u32 * (255 - a)) / 255).min(255) as u8
            };
            *dst = PremultipliedColorU8::from_rgba(
                mix(s.r, background.r),
                mix(s.g, background.g),
                mix(s.b, background.b),
                255,
            )
            .unwrap_or_else(|| {
                PremultipliedColorU8::from_rgba(0, 0, 0, 255).expect("opaque black")
            });
        }
        Snapshot { pixels: out }
    }

    /// The underlying pixmap, for cropping and saving.
    #[inline]
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixels
    }
}
