//! Painter handed to custom controls (Godot's `_draw()`).

use std::sync::Arc;

use neko_render::epaint::emath::Align2;
use neko_render::epaint::{ClippedShape, Color32, CornerRadius, FontFamily, FontId, Galley, Shape, Stroke, StrokeKind};

use crate::geom::{Pos2, Rect};
use crate::text::TextServer;

pub struct Painter<'a> {
    pub(crate) shapes: &'a mut Vec<ClippedShape>,
    pub(crate) clip: Rect,
    pub(crate) text: &'a mut TextServer,
}

impl Painter<'_> {
    pub fn clip_rect(&self) -> Rect {
        self.clip
    }

    pub fn add(&mut self, shape: impl Into<Shape>) {
        self.shapes.push(ClippedShape::new(self.clip, shape.into()));
    }

    pub fn line_segment(&mut self, points: [Pos2; 2], stroke: impl Into<Stroke>) {
        self.add(Shape::line_segment(points, stroke));
    }

    pub fn circle_filled(&mut self, center: Pos2, radius: f32, color: impl Into<Color32>) {
        self.add(Shape::circle_filled(center, radius, color));
    }

    pub fn circle_stroke(&mut self, center: Pos2, radius: f32, stroke: impl Into<Stroke>) {
        self.add(Shape::circle_stroke(center, radius, stroke));
    }

    pub fn rect_filled(&mut self, rect: Rect, radius: impl Into<CornerRadius>, color: impl Into<Color32>) {
        self.add(Shape::rect_filled(rect, radius, color));
    }

    pub fn rect_stroke(&mut self, rect: Rect, radius: impl Into<CornerRadius>, stroke: impl Into<Stroke>, kind: StrokeKind) {
        self.add(Shape::rect_stroke(rect, radius, stroke, kind));
    }

    pub fn galley(&mut self, pos: Pos2, galley: Arc<Galley>, color: Color32) {
        self.add(Shape::galley(pos, galley, color));
    }

    /// Lay out and draw one line of text anchored at `pos`. Returns its rect.
    pub fn text(&mut self, pos: Pos2, anchor: Align2, text: &str, font: FontId, color: Color32) -> Rect {
        let galley = match &font.family {
            FontFamily::Monospace => self.text.layout_mono(text, font.size, color),
            FontFamily::Name(n) if n.as_ref() == "bold" => self.text.layout_bold(text, font.size, color),
            _ => self.text.layout(text, font.size, color),
        };
        let rect = anchor.anchor_size(pos, galley.size());
        self.galley(rect.min, galley, color);
        rect
    }

    pub fn circle(&mut self, center: Pos2, radius: f32, fill: Color32, stroke: impl Into<Stroke>) {
        self.add(Shape::circle_filled(center, radius, fill));
        self.add(Shape::circle_stroke(center, radius, stroke));
    }

    pub fn rect(&mut self, rect: Rect, radius: impl Into<CornerRadius> + Copy, fill: Color32, stroke: impl Into<Stroke>, kind: StrokeKind) {
        self.add(Shape::rect_filled(rect, radius, fill));
        self.add(Shape::rect_stroke(rect, radius, stroke, kind));
    }

    /// A line from `origin` along `vec` with an arrow head.
    pub fn arrow(&mut self, origin: Pos2, vec: crate::Vec2, stroke: impl Into<Stroke>) {
        let stroke = stroke.into();
        let tip = origin + vec;
        let len = vec.length().max(1e-3);
        let dir = vec / len;
        let head = (len * 0.25).min(10.0);
        let side = crate::vec2(-dir.y, dir.x) * head * 0.5;
        self.line_segment([origin, tip], stroke);
        self.line_segment([tip, tip - dir * head + side], stroke);
        self.line_segment([tip, tip - dir * head - side], stroke);
    }

    /// Closed dashed outline through `pts`.
    pub fn dashed(&mut self, pts: &[Pos2], stroke: impl Into<Stroke>, dash: f32, gap: f32) {
        let stroke = stroke.into();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            for s in Shape::dashed_line(&[a, b], stroke, dash, gap) {
                self.add(s);
            }
        }
    }

    pub fn image(&mut self, img: crate::Image, rect: Rect) {
        self.add(Shape::image(img.texture, rect, img.uv, img.tint));
    }

    pub fn text_server(&mut self) -> &mut TextServer {
        self.text
    }
}
