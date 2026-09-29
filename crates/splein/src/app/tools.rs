use crate::domain::canvas::{DrawingElement, Rgba};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::{Aabb, Vec2};

pub fn build_shape_preview(tool: ActiveTool, start: Vec2, current: Vec2, color: Rgba) -> Option<DrawingElement> {
    match tool {
        ActiveTool::Line => {
            let mut aabb = Aabb::from_point(start);
            aabb.expand_with_point(current);
            Some(DrawingElement::Line { start, end: current, color, width: 3.5, aabb })
        }
        ActiveTool::Rect => {
            let min = Vec2::new(start.x.min(current.x), start.y.min(current.y));
            let max = Vec2::new(start.x.max(current.x), start.y.max(current.y));
            Some(DrawingElement::Rect { min, max, color, width: 3.0, aabb: Aabb::new(min, max) })
        }
        ActiveTool::Ellipse => {
            let center = Vec2::new((start.x + current.x) * 0.5, (start.y + current.y) * 0.5);
            let rx = (current.x - start.x).abs() * 0.5;
            let ry = (current.y - start.y).abs() * 0.5;
            let aabb = Aabb::new(Vec2::new(center.x - rx, center.y - ry), Vec2::new(center.x + rx, center.y + ry));
            Some(DrawingElement::Ellipse { center, rx, ry, color, width: 3.0, aabb })
        }
        _ => None,
    }
}
