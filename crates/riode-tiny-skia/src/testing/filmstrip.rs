// Single responsibility: Compositing several frames into one labelled strip for visual review.

use tiny_skia::{Pixmap, PremultipliedColorU8};

use crate::testing::snapshot::Snapshot;

/// A 3x5 bitmap font for the digits and a few marks.
///
/// A strip with no labels is much harder to reason about than one that says which frame it
/// is, and pulling in a font for six glyphs would be absurd.
mod digits {
    /// Row bitmaps, five rows of three bits, most significant bit leftmost.
    pub const GLYPHS: [[u8; 5]; 11] = [
        [0b111, 0b101, 0b101, 0b101, 0b111], // 0
        [0b010, 0b110, 0b010, 0b010, 0b111], // 1
        [0b111, 0b001, 0b111, 0b100, 0b111], // 2
        [0b111, 0b001, 0b111, 0b001, 0b111], // 3
        [0b101, 0b101, 0b111, 0b001, 0b001], // 4
        [0b111, 0b100, 0b111, 0b001, 0b111], // 5
        [0b111, 0b100, 0b111, 0b101, 0b111], // 6
        [0b111, 0b001, 0b010, 0b010, 0b010], // 7
        [0b111, 0b101, 0b111, 0b101, 0b111], // 8
        [0b111, 0b101, 0b111, 0b001, 0b111], // 9
        [0b000, 0b010, 0b101, 0b000, 0b000], // ·
    ];
}

/// Fully transparent, for pixels a source frame does not cover.
fn transparent() -> PremultipliedColorU8 {
    PremultipliedColorU8::from_rgba(0, 0, 0, 0)
        .unwrap_or(PremultipliedColorU8::from_rgba(0, 0, 0, 255).expect("black premultiplies"))
}

/// Label ink, dark enough to read on a light page.
fn ink() -> PremultipliedColorU8 {
    PremultipliedColorU8::from_rgba(20, 20, 20, 255)
        .unwrap_or(PremultipliedColorU8::from_rgba(0, 0, 0, 255).expect("black premultiplies"))
}

/// Where the frames sit relative to each other.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Strip {
    /// Frames side by side, left to right.
    #[default]
    Row,
    /// Frames stacked, top to bottom.
    Column,
}

/// Composites snapshots into a single image, optionally labelling each frame.
#[derive(Clone)]
pub struct Filmstrip {
    frames: Vec<Snapshot>,
    labels: Vec<String>,
    order: Strip,
    scale: u32,
}

impl Filmstrip {
    /// Starts a strip from labelled frames.
    pub fn new(frames: Vec<(u64, Snapshot)>) -> Self {
        let mut labels = Vec::with_capacity(frames.len());
        let mut shots = Vec::with_capacity(frames.len());
        for (index, snapshot) in frames {
            labels.push(index.to_string());
            shots.push(snapshot);
        }
        Self {
            frames: shots,
            labels,
            order: Strip::Row,
            scale: 1,
        }
    }

    /// Starts a strip from unlabelled frames, numbered from zero.
    pub fn from_snapshots(frames: Vec<Snapshot>) -> Self {
        let labels = (0..frames.len()).map(|i| i.to_string()).collect();
        Self {
            frames,
            labels,
            order: Strip::Row,
            scale: 1,
        }
    }

    /// Lays the frames out in a column instead of a row.
    ///
    /// Takes and returns a clone so a strip can be arranged two ways without rebuilding it.
    pub fn column(&self) -> Self {
        Self {
            order: Strip::Column,
            ..self.clone()
        }
    }

    /// Halves or doubles each frame, to fit several into one readable image.
    pub fn scaled(&self, scale: u32) -> Self {
        Self {
            scale: scale.max(1),
            ..self.clone()
        }
    }

    /// Number of frames in the strip.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the strip has no frames.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Composites the strip.
    pub fn render(&self) -> Pixmap {
        let Some(first) = self.frames.first() else {
            return Pixmap::new(1, 1).expect("alloc");
        };
        let fw = (first.width() / self.scale).max(1);
        let fh = (first.height() / self.scale).max(1);
        let label_h: u32 = if self.labels.is_empty() { 0 } else { 8 };
        let gap = 4u32;

        let (width, height) = match self.order {
            Strip::Row => (
                fw * self.frames.len() as u32 + gap * (self.frames.len() as u32 - 1),
                fh + label_h,
            ),
            Strip::Column => (
                fw,
                (fh + label_h) * self.frames.len() as u32 + gap * (self.frames.len() as u32 - 1),
            ),
        };

        let mut out = Pixmap::new(width.max(1), height.max(1)).expect("alloc");
        out.fill(tiny_skia::Color::TRANSPARENT);

        for (i, frame) in self.frames.iter().enumerate() {
            let (ox, oy) = match self.order {
                Strip::Row => ((fw + gap) * i as u32, 0),
                Strip::Column => (0, (fh + label_h + gap) * i as u32),
            };
            blit(frame, &mut out, ox, oy, self.scale);
            if label_h > 0 {
                if let Some(label) = self.labels.get(i) {
                    draw_text(&mut out, label, ox + 1, oy + fh + 2);
                }
            }
        }
        out
    }

    /// Composites and writes the strip as a PNG.
    pub fn save(&self, path: &str) {
        if let Err(err) = self.render().save_png(path) {
            panic!("could not write filmstrip to {path}: {err}");
        }
    }
}

/// Copies a frame into `out`, nearest-neighbour scaled.
fn blit(frame: &Snapshot, out: &mut Pixmap, ox: u32, oy: u32, scale: u32) {
    let src = frame.pixmap();
    let fw = (src.width() / scale).max(1);
    let fh = (src.height() / scale).max(1);
    let mut target = out.as_mut();
    for y in 0..fh {
        let sy = (y * scale).min(src.height().saturating_sub(1));
        for x in 0..fw {
            let sx = (x * scale).min(src.width().saturating_sub(1));
            let tx = ox + x;
            let ty = oy + y;
            if tx >= target.width() || ty >= target.height() {
                continue;
            }
            let px = match src.pixel(sx, sy) {
                Some(p) => p,
                None => transparent(),
            };
            let idx = (ty as usize) * (target.width() as usize) + (tx as usize);
            if let Some(cell) = target.pixels_mut().get_mut(idx) {
                *cell = px;
            }
        }
    }
}

/// Draws a small label, scaling it so the digits stay legible when a strip is downscaled.
fn draw_text(out: &mut Pixmap, label: &str, x: u32, y: u32) {
    let mut target = out.as_mut();
    let mut cursor = x;
    for ch in label.chars() {
        let Some(digit) = ch.to_digit(10) else {
            cursor += 3;
            continue;
        };
        let glyph = &digits::GLYPHS[digit as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) != 0 {
                    let (px, py) = (cursor + col as u32, y + row as u32);
                    if px < target.width() && py < target.height() {
                        let idx = (py as usize) * (target.width() as usize) + (px as usize);
                        if let Some(cell) = target.pixels_mut().get_mut(idx) {
                            *cell = ink();
                        }
                    }
                }
            }
        }
        cursor += 4;
    }
}
