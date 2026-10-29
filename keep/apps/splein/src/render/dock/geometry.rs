// Holds computed layout boxes and bounds for dock widgets and hit-testing.

use crate::domain::geometry::Vec2;

#[derive(Debug, Clone, Copy, Default)]
pub struct Box2D {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Box2D {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DockGeometry {
    pub container: Box2D,
    pub grip: Box2D,
    pub tools: [Box2D; 6],
    pub divider: Box2D,
    pub actions: [Box2D; 2],
    pub submenu: Option<Box2D>,
    pub submenu_shapes: [Box2D; 4],
}
