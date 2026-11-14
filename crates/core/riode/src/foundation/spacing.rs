// Single responsibility: External margin, internal padding, and inter-element gap spatial models.

/// External spacing offset applied around element bounding boxes.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Margin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margin {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
    pub const fn all(val: f32) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }
    pub const fn symmetric(y: f32, x: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn top(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
    pub const fn right(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: 0.0,
        }
    }
    pub const fn bottom(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }
    pub const fn left(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: val,
        }
    }

    pub const fn xy(x: f32, y: f32) -> Self {
        Self::symmetric(y, x)
    }

    pub const fn x(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: val,
        }
    }
    pub const fn y(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }

    pub const fn sides(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn merged(self, over: Self) -> Self {
        Self {
            top: if over.top != 0.0 { over.top } else { self.top },
            right: if over.right != 0.0 { over.right } else { self.right },
            bottom: if over.bottom != 0.0 { over.bottom } else { self.bottom },
            left: if over.left != 0.0 { over.left } else { self.left },
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Internal spacing inset applied inside an element's bounding box.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
    pub const fn all(val: f32) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }
    pub const fn symmetric(y: f32, x: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn top(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
    pub const fn right(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: 0.0,
        }
    }
    pub const fn bottom(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }
    pub const fn left(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: val,
        }
    }

    pub const fn xy(x: f32, y: f32) -> Self {
        Self::symmetric(y, x)
    }

    pub const fn x(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: val,
        }
    }
    pub const fn y(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }

    pub const fn sides(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn merged(self, over: Self) -> Self {
        Self {
            top: if over.top != 0.0 { over.top } else { self.top },
            right: if over.right != 0.0 { over.right } else { self.right },
            bottom: if over.bottom != 0.0 { over.bottom } else { self.bottom },
            left: if over.left != 0.0 { over.left } else { self.left },
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

impl From<Margin> for Padding {
    fn from(m: Margin) -> Self {
        Self {
            top: m.top,
            right: m.right,
            bottom: m.bottom,
            left: m.left,
        }
    }
}

impl From<f32> for Padding {
    fn from(val: f32) -> Self {
        Self::all(val)
    }
}

impl From<i32> for Padding {
    fn from(val: i32) -> Self {
        Self::all(val as f32)
    }
}

impl From<u32> for Padding {
    fn from(val: u32) -> Self {
        Self::all(val as f32)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Gap {
    Fixed(f32),
    Split { main: f32, cross: f32 },
}

impl Gap {
    pub fn main_px(self) -> f32 {
        match self {
            Gap::Fixed(px) if px.is_finite() => px.max(0.0),
            Gap::Fixed(_) => 0.0,
            Gap::Split { main, .. } if main.is_finite() => main.max(0.0),
            Gap::Split { .. } => 0.0,
        }
    }

    pub fn cross_px(self) -> f32 {
        match self {
            Gap::Fixed(px) if px.is_finite() => px.max(0.0),
            Gap::Fixed(_) => 0.0,
            Gap::Split { cross, .. } if cross.is_finite() => cross.max(0.0),
            Gap::Split { .. } => 0.0,
        }
    }

    pub fn with_main(self, main: f32) -> Self {
        Gap::Split { main, cross: self.cross_px() }
    }

    pub fn with_cross(self, cross: f32) -> Self {
        Gap::Split { main: self.main_px(), cross }
    }
}

impl Default for Gap {
    fn default() -> Self {
        Self::Fixed(0.0)
    }
}

impl From<f32> for Gap {
    fn from(val: f32) -> Self {
        Self::Fixed(val)
    }
}

impl From<&str> for Gap {
    fn from(s: &str) -> Self {
        let s = s.trim();
        if s.ends_with("px") {
            if let Ok(v) = s.trim_end_matches("px").trim().parse::<f32>() {
                return Gap::Fixed(v);
            }
        }
        if let Ok(v) = s.parse::<f32>() {
            return Gap::Fixed(v);
        }
        Gap::Fixed(0.0)
    }
}

impl From<String> for Gap {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&str> for Margin {
    fn from(s: &str) -> Self {
        let s = s.trim();
        if s.ends_with("px") {
            let t = s.trim_end_matches("px").trim();
            if let Ok(v) = t.parse::<f32>() {
                return Margin::all(v);
            }
        }
        if let Ok(v) = s.parse::<f32>() {
            return Margin::all(v);
        }
        Margin::ZERO
    }
}

impl From<String> for Margin {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&str> for Padding {
    fn from(s: &str) -> Self {
        let s = s.trim();
        if s.ends_with("px") {
            let t = s.trim_end_matches("px").trim();
            if let Ok(v) = t.parse::<f32>() {
                return Padding::all(v);
            }
        }
        if let Ok(v) = s.parse::<f32>() {
            return Padding::all(v);
        }
        Padding::ZERO
    }
}

impl From<String> for Padding {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&str> for crate::foundation::sizing::Size {
    fn from(s: &str) -> Self {
        let s = s.trim();
        if s == "fill" || s == "full" {
            return crate::foundation::sizing::Size::Fill;
        }
        if s == "auto" || s == "hug" || s == "fit" {
            return crate::foundation::sizing::Size::Fit;
        }
        if let Some(pct) = s.strip_suffix('%') {
            if let Ok(v) = pct.trim().parse::<f32>() {
                return crate::foundation::sizing::Size::Percent(v / 100.0);
            }
        }
        if s.ends_with("px") {
            let t = s.trim_end_matches("px").trim();
            if let Ok(v) = t.parse::<f32>() {
                return crate::foundation::sizing::Size::Fixed(v);
            }
        }
        if let Ok(v) = s.parse::<f32>() {
            return crate::foundation::sizing::Size::Fixed(v);
        }
        crate::foundation::sizing::Size::Fit
    }
}

impl From<String> for crate::foundation::sizing::Size {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<u32> for Gap {
    fn from(v: u32) -> Self {
        Gap::Fixed(v as f32)
    }
}

impl From<i32> for Gap {
    fn from(v: i32) -> Self {
        Gap::Fixed(v as f32)
    }
}

impl From<f32> for Margin {
    fn from(v: f32) -> Self {
        Margin::all(v)
    }
}
