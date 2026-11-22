//! The 2D viewport (Godot's CanvasItemEditor), drawn on a neko-ui Canvas.

use neko::core::{ClassDb, NodeId, Variant};
use neko::platform::MouseButton;
use neko_ui::{Align2, CanvasInput, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::{Doc, Editor, GAME_SIZE, Tool};
use crate::theme::*;

const SNAP_2D: f32 = 8.0;
/// `editors/2d/ruler_width` default.
const RULER: f32 = 16.0;
/// `editors/2d/zoom_speed_factor` default.
const ZOOM_FACTOR: f32 = 1.1;

/// The zoom widget and Center View button over the top-left corner
/// (EditorZoomWidget + button_center_view), as hit rects.
struct Overlay {
    center: Rect,
    minus: Rect,
    reset: Rect,
    plus: Rect,
}

fn overlay(rect: Rect) -> Overlay {
    let o = rect.left_top() + vec2(RULER + 8.0, RULER + 8.0);
    let b = |x: f32, w: f32| Rect::from_min_size(o + vec2(x, 0.0), vec2(w, 22.0));
    Overlay { center: b(0.0, 22.0), minus: b(28.0, 22.0), reset: b(52.0, 58.0), plus: b(112.0, 22.0) }
}

/// EditorZoomWidget::_update_zoom_label.
fn zoom_text(zoom: f32) -> String {
    let pct = zoom * 100.0;
    if zoom >= 10.0 {
        format!("{pct:.0} %")
    } else if zoom >= 0.1 {
        format!("{pct:.1} %")
    } else {
        format!("{pct:.2} %")
    }
}

/// EditorZoomWidget::set_zoom_by_increments (non-integer mode): zoom levels are
/// powers of the zoom speed factor.
fn zoom_steps(zoom: f32, steps: i32) -> f32 {
    let current = (zoom.ln() / ZOOM_FACTOR.ln()).round();
    ZOOM_FACTOR.powf(current + steps as f32).clamp(1.0 / 128.0, 128.0)
}

/// CanvasItemEditor::_draw_rulers, without a snapping grid: a major
/// graduation every `basic_rule` units, split in 2 then 5.
fn draw_rulers(p: &mut Painter, rect: Rect, origin: Pos2, zoom: f32) {
    let bg = Color32::from_rgba_unmultiplied(29, 29, 29, 204);
    let font_color = Color32::from_rgba_unmultiplied(224, 224, 224, 230);
    let grad = Color32::from_gray(127);
    let tick = RULER / 15.0;
    let mut rule = 100.0f32;
    let mut i = 0;
    while rule * zoom > 100.0 * tick {
        rule /= if i % 2 == 1 { 5.0 } else { 2.0 };
        i += 1;
    }
    i = 0;
    while rule * zoom < 60.0 * tick {
        rule *= if i % 2 == 1 { 2.0 } else { 5.0 };
        i += 1;
    }
    let minor = rule / 10.0;
    let font = 8.0f32.max((8.0 * RULER / 15.0).round());
    let label = |v: f32| if v == v.trunc() { format!("{v:.0}") } else { format!("{v:.1}") };
    let line = Stroke::new(1.0, grad);
    // Top ruler.
    p.rect_filled(Rect::from_min_max(rect.left_top() + vec2(RULER, 0.0), pos2(rect.right(), rect.top() + RULER)), 0, bg);
    let first = ((rect.left() + RULER - origin.x) / zoom / minor).ceil() as i64;
    let last = ((rect.right() - origin.x) / zoom / minor) as i64;
    for i in first..=last {
        let x = (origin.x + i as f32 * minor * zoom).round();
        let y0 = if i % 10 == 0 {
            p.text(pos2(x + 2.0, rect.top() + 1.0), Align2::LEFT_TOP, &label(i as f32 * minor), FontId::proportional(font), font_color);
            0.0
        } else if i % 5 == 0 {
            RULER * 0.33
        } else {
            RULER * 0.75
        };
        p.line_segment([pos2(x, rect.top() + y0), pos2(x, rect.top() + RULER)], line);
    }
    // Left ruler, labels rotated a quarter turn counter-clockwise.
    p.rect_filled(Rect::from_min_max(rect.left_top() + vec2(0.0, RULER), pos2(rect.left() + RULER, rect.bottom())), 0, bg);
    let first = ((rect.top() + RULER - origin.y) / zoom / minor).ceil() as i64;
    let last = ((rect.bottom() - origin.y) / zoom / minor) as i64;
    for i in first..=last {
        let y = (origin.y + i as f32 * minor * zoom).round();
        let x0 = if i % 10 == 0 {
            let galley = p.text_server().layout(&label(i as f32 * minor), font, font_color);
            p.add(Shape::Text(neko::render::epaint::TextShape::new(pos2(rect.left() + 1.0, y - 2.0), galley, font_color).with_angle(-std::f32::consts::FRAC_PI_2)));
            0.0
        } else if i % 5 == 0 {
            RULER * 0.33
        } else {
            RULER * 0.75
        };
        p.line_segment([pos2(rect.left() + x0, y), pos2(rect.left() + RULER, y)], line);
    }
    p.rect_filled(Rect::from_min_size(rect.left_top(), Vec2::splat(RULER)), 0, grad);
}

fn draw_overlay(p: &mut Painter, rect: Rect, zoom: f32, icons: crate::node_icons::Icons, hover: Option<Pos2>) {
    let o = overlay(rect);
    let fg = Color32::from_gray(224);
    for (r, icon) in [(o.center, "CenterView"), (o.minus, "ZoomLess"), (o.plus, "ZoomMore")] {
        let hot = hover.is_some_and(|h| r.contains(h));
        if hot {
            p.rect_filled(r, 3, Color32::from_white_alpha(20));
        }
        if let Some(img) = icons.get(icon) {
            p.image(img.tinted(if hot { Color32::WHITE } else { fg }), Rect::from_center_size(r.center(), vec2(16.0, 16.0)));
        }
    }
    if hover.is_some_and(|h| o.reset.contains(h)) {
        p.rect_filled(o.reset, 3, Color32::from_white_alpha(20));
    }
    p.text(o.reset.center(), Align2::CENTER_CENTER, &zoom_text(zoom), FontId::proportional(12.0), fg);
}

/// Corners of a w*h box at `origin` (its center when `centered`), rotated by `deg` around `pivot`.
fn quad(pivot: Pos2, origin: Pos2, size: Vec2, centered: bool, deg: f32) -> Vec<Pos2> {
    let min = if centered { origin - size / 2.0 } else { origin };
    let pts = [min, min + vec2(size.x, 0.0), min + size, min + vec2(0.0, size.y)];
    let (s, c) = deg.to_radians().sin_cos();
    pts.iter()
        .map(|p| {
            let d = *p - pivot;
            pivot + vec2(d.x * c - d.y * s, d.x * s + d.y * c)
        })
        .collect()
}

fn bounds(pts: &[Pos2]) -> Rect {
    let mut r = Rect::NOTHING;
    for p in pts {
        r.extend_with(*p);
    }
    r
}

fn str_prop(n: &neko::core::Node, prop: &str) -> String {
    n.get(prop).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Draw the scene; returns the pickable rect of every drawn node.
fn draw(p: &mut Painter, rect: Rect, doc: &Doc, show_grid: bool, show_collisions: bool) -> Vec<(NodeId, Rect)> {
    p.rect_filled(rect, CornerRadius::ZERO, VIEWPORT);
    let zoom = doc.zoom;
    let origin = rect.center() + doc.pan;
    if show_grid {
        let mut step = 32.0 * zoom;
        while step < 12.0 {
            step *= 2.0;
        }
        let grid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        let mut x = origin.x - ((origin.x - rect.left()) / step).floor() * step;
        while x < rect.right() {
            p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], grid);
            x += step;
        }
        let mut y = origin.y - ((origin.y - rect.top()) / step).floor() * step;
        while y < rect.bottom() {
            p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], grid);
            y += step;
        }
    }
    p.line_segment([pos2(rect.left(), origin.y), pos2(rect.right(), origin.y)], Stroke::new(1.0, AXIS_X));
    p.line_segment([pos2(origin.x, rect.top()), pos2(origin.x, rect.bottom())], Stroke::new(1.0, AXIS_Y));
    p.rect_stroke(Rect::from_min_size(origin, GAME_SIZE * zoom), CornerRadius::ZERO, Stroke::new(1.0, Color32::from_rgba_unmultiplied(102, 102, 255, 102)), StrokeKind::Inside);
    let mut hits = Vec::new();
    if !doc.has_root {
        return hits;
    }
    let db = ClassDb::get();
    for (id, _) in doc.scene.walk() {
        let Some(n) = doc.scene.get(id) else { continue };
        let is_2d = db.inherits(n.class, "Node2D");
        let is_ui = db.inherits(n.class, "Control");
        if (!is_2d && !is_ui) || n.get("visible").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let sel = doc.selected == Some(id);
        let col = if sel { ACCENT } else if is_ui { CUI } else { C2D };
        let pt = origin + Vec2::from(doc.scene.global_position_2d(id)) * zoom;
        let scale = n.get("scale").and_then(|v| v.as_vec2()).unwrap_or([1.0, 1.0]);
        let rot = n.get("rotation_degrees").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let width = if sel { 2.0 } else { 1.0 };
        let hit = match n.class {
            "Sprite2D" | "AnimatedSprite2D" => {
                let size = vec2(64.0 * scale[0], 64.0 * scale[1]) * zoom;
                let centered = n.get("centered").and_then(|v| v.as_bool()).unwrap_or(true);
                let pts = quad(pt, pt, size, centered, rot);
                p.add(Shape::convex_polygon(pts.clone(), col.gamma_multiply(0.18), Stroke::new(width, col)));
                if size.x > 50.0 {
                    let tex = str_prop(n, "texture");
                    let label = if tex.is_empty() { "no texture".to_string() } else { tex.rsplit('/').next().unwrap_or("").to_string() };
                    p.text(bounds(&pts).center(), Align2::CENTER_CENTER, &label, FontId::proportional(10.0), MUTED);
                }
                bounds(&pts)
            }
            "CollisionShape2D" => {
                let s = n.get("size").and_then(|v| v.as_vec2()).unwrap_or([32.0, 32.0]);
                let size = vec2(s[0] * scale[0], s[1] * scale[1]) * zoom;
                let c = if sel { ACCENT } else { Color32::from_rgb(0x4f, 0xc3, 0xf7) };
                if show_collisions || sel {
                    if str_prop(n, "shape") == "circle" {
                        p.circle(pt, size.x / 2.0, c.gamma_multiply(0.15), Stroke::new(1.5, c));
                    } else {
                        p.add(Shape::convex_polygon(quad(pt, pt, size, true, rot), c.gamma_multiply(0.15), Stroke::new(1.5, c)));
                    }
                }
                Rect::from_center_size(pt, size)
            }
            "Camera2D" => {
                let zm = n.get("zoom").and_then(|v| v.as_vec2()).unwrap_or([1.0, 1.0]);
                let size = vec2(GAME_SIZE.x / zm[0].max(0.01), GAME_SIZE.y / zm[1].max(0.01)) * zoom;
                let c = if sel { ACCENT } else { Color32::from_rgb(0xc7, 0x9b, 0xff) };
                p.dashed(&quad(pt, pt, size, true, 0.0), Stroke::new(1.5, c), 6.0, 4.0);
                p.circle_stroke(pt, 6.0, Stroke::new(1.5, c));
                Rect::from_center_size(pt, Vec2::splat(16.0))
            }
            _ if is_ui => {
                let s = n.get("size").and_then(|v| v.as_vec2()).unwrap_or([100.0, 30.0]);
                let r = Rect::from_min_size(pt, vec2(s[0], s[1]) * zoom);
                p.rect_stroke(r, CornerRadius::same(2), Stroke::new(width, col), StrokeKind::Inside);
                let text = str_prop(n, "text");
                if !text.is_empty() {
                    p.text(r.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, &text, FontId::proportional((14.0 * zoom).clamp(6.0, 40.0)), FG);
                }
                r
            }
            _ => {
                p.circle_filled(pt, if sel { 5.0 } else { 3.5 }, col);
                if db.inherits(n.class, "PhysicsBody2D") || n.class == "Area2D" {
                    p.line_segment([pt - vec2(8.0, 0.0), pt + vec2(8.0, 0.0)], Stroke::new(1.0, col));
                    p.line_segment([pt - vec2(0.0, 8.0), pt + vec2(0.0, 8.0)], Stroke::new(1.0, col));
                }
                Rect::from_center_size(pt, Vec2::splat(18.0))
            }
        };
        if sel {
            // Move gizmo: red X and green Y arrows, as in Godot.
            p.arrow(pt, vec2(40.0, 0.0), Stroke::new(2.0, AXIS_X));
            p.arrow(pt, vec2(0.0, -40.0), Stroke::new(2.0, AXIS_Y));
            p.text(hit.left_top() + vec2(0.0, -3.0), Align2::LEFT_BOTTOM, &n.name, FontId::proportional(11.0), ACCENT);
        }
        hits.push((id, hit));
    }
    hits
}

impl Editor {
    pub(super) fn paint_viewport2d(&mut self) {
        let doc = &self.docs[self.current];
        let (grid, coll) = (self.show_grid, self.show_collisions);
        let mut hits = Vec::new();
        let (icons, hover) = (self.icons, self.ui.mouse_pos());
        self.ui.paint_canvas(self.w.viewport, |p, rect| {
            hits = draw(p, rect, doc, grid, coll);
            draw_rulers(p, rect, rect.center() + doc.pan, doc.zoom);
            draw_overlay(p, rect, doc.zoom, icons, hover);
        });
        self.hits2d = hits;
    }

    fn pick2d(&self, pos: Pos2) -> Option<NodeId> {
        self.hits2d.iter().rev().find(|(_, r)| r.expand(3.0).contains(pos)).map(|(id, _)| *id)
    }

    pub(super) fn viewport2d_input(&mut self, input: CanvasInput) {
        let rect = self.ui.rect(self.w.viewport);
        match input {
            CanvasInput::Wheel { pos, delta } if delta.y != 0.0 => self.zoom2d(pos, if delta.y > 0.0 { 1 } else { -1 }),
            CanvasInput::Press { button: MouseButton::Left, pos } if overlay(rect).center.contains(pos) => {
                // Center View: frame the project's viewport rect.
                let doc = self.doc_mut();
                doc.pan = -GAME_SIZE * doc.zoom / 2.0;
            }
            CanvasInput::Press { button: MouseButton::Left, pos } if overlay(rect).minus.contains(pos) => self.zoom2d(rect.center(), -6),
            CanvasInput::Press { button: MouseButton::Left, pos } if overlay(rect).plus.contains(pos) => self.zoom2d(rect.center(), 6),
            CanvasInput::Press { button: MouseButton::Left, pos } if overlay(rect).reset.contains(pos) => {
                let doc = self.doc_mut();
                let world = -doc.pan / doc.zoom;
                doc.zoom = 1.0;
                doc.pan = -world;
            }
            CanvasInput::Drag { button: MouseButton::Middle | MouseButton::Right, delta, .. } => self.doc_mut().pan += delta,
            CanvasInput::Press { button: MouseButton::Left, pos } => {
                if !self.doc().has_root {
                    return;
                }
                let picked = self.pick2d(pos);
                let root = self.doc().scene.root();
                self.doc_mut().selected = picked.or(Some(root));
                self.dragging_node = picked.is_some();
                self.drag_moved = false;
                self.drag_acc = Vec2::ZERO;
            }
            CanvasInput::Drag { button: MouseButton::Left, delta, .. } if self.dragging_node => {
                if !self.drag_moved {
                    // One undo step per drag, taken when the node first moves.
                    self.drag_moved = true;
                    self.doc_mut().checkpoint(None);
                }
                self.drag2d(delta);
            }
            CanvasInput::Release { button: MouseButton::Left, .. } => self.dragging_node = false,
            _ => {}
        }
    }

    /// Zoom by `steps` levels keeping the point under `pos` still (CanvasItemEditor::_zoom_on_position).
    fn zoom2d(&mut self, pos: Pos2, steps: i32) {
        let rect = self.ui.rect(self.w.viewport);
        let doc = self.doc_mut();
        let origin = rect.center() + doc.pan;
        let world = (pos - origin) / doc.zoom;
        doc.zoom = zoom_steps(doc.zoom, steps);
        doc.pan = pos - rect.center() - world * doc.zoom;
    }

    fn drag2d(&mut self, delta: Vec2) {
        let (tool, snap) = (self.tool, self.snap);
        let zoom = self.doc().zoom;
        self.drag_acc += delta / zoom;
        let acc = self.drag_acc;
        let doc = &mut self.docs[self.current];
        let Some(id) = doc.selected else { return };
        let Some(n) = doc.scene.get_mut(id) else { return };
        match tool {
            Tool::Select | Tool::Move => {
                if let Some([x, y]) = n.get("position").and_then(|v| v.as_vec2()) {
                    let (mut nx, mut ny) = (x + acc.x, y + acc.y);
                    if snap {
                        nx = (nx / SNAP_2D).round() * SNAP_2D;
                        ny = (ny / SNAP_2D).round() * SNAP_2D;
                    } else {
                        nx = nx.round();
                        ny = ny.round();
                    }
                    // Keep the unsnapped remainder so slow drags still move.
                    self.drag_acc = acc - vec2(nx - x, ny - y);
                    let _ = n.set("position", Variant::Vec2([nx, ny]));
                }
            }
            Tool::Rotate => {
                if let Some(r) = n.get("rotation_degrees").and_then(|v| v.as_f64()) {
                    let step = if snap { 15.0 } else { 1.0 };
                    let new = ((r + delta.x as f64 * 0.5) / step).round() * step;
                    let _ = n.set("rotation_degrees", Variant::Float(new));
                }
            }
            Tool::Scale => {
                if let Some([sx, sy]) = n.get("scale").and_then(|v| v.as_vec2()) {
                    let f = (1.0 + delta.x * 0.01).max(0.01);
                    let _ = n.set("scale", Variant::Vec2([(sx * f * 100.0).round() / 100.0, (sy * f * 100.0).round() / 100.0]));
                }
            }
        }
    }
}
