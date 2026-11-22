//! Small geometry helpers.

pub use neko_render::epaint::{Pos2, Rect, Vec2, pos2, vec2};

/// Padding or margins on four sides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Sides {
    pub const ZERO: Sides = Sides { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };

    pub const fn all(v: f32) -> Self {
        Sides { left: v, top: v, right: v, bottom: v }
    }

    pub const fn symmetric(x: f32, y: f32) -> Self {
        Sides { left: x, top: y, right: x, bottom: y }
    }

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Sides { left, top, right, bottom }
    }

    pub fn size(&self) -> Vec2 {
        vec2(self.left + self.right, self.top + self.bottom)
    }

    pub fn shrink(&self, r: Rect) -> Rect {
        Rect::from_min_max(pos2(r.min.x + self.left, r.min.y + self.top), pos2((r.max.x - self.right).max(r.min.x + self.left), (r.max.y - self.bottom).max(r.min.y + self.top)))
    }
}
