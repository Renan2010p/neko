//! Small editor icons drawn with shapes, so they need no font glyphs or image files.

use neko_ui::{Color32, CornerRadius, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

/// Icons drawn with shapes for screens that do not use Godot's atlas.
#[allow(dead_code)] // the full set mirrors the atlas so screens can switch freely
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Import,
    Search,
    Folder,
    Play,
    Rename,
    Duplicate,
    Tag,
    Trash,
    Broom,
    Settings,
    List,
    Download,
    Info,
    Ws2D,
    Ws3D,
    Script,
    Game,
    Pause,
    Stop,
    PlayScene,
    Movie,
    ToolSelect,
    ToolMove,
    ToolRotate,
    ToolScale,
    Snap,
    Grid,
    Close,
    Scene,
    File,
    Star,
    StarFilled,
}

/// Anything icons can be drawn on: egui's painter or neko-ui's.
pub trait Canvas {
    fn line_segment(&mut self, points: [Pos2; 2], stroke: Stroke);
    fn circle_filled(&mut self, center: Pos2, radius: f32, color: Color32);
    fn circle_stroke(&mut self, center: Pos2, radius: f32, stroke: Stroke);
    fn rect_filled(&mut self, rect: Rect, radius: CornerRadius, color: Color32);
    fn rect_stroke(&mut self, rect: Rect, radius: CornerRadius, stroke: Stroke, kind: StrokeKind);
    fn add(&mut self, shape: Shape);
}

impl Canvas for neko_ui::Painter<'_> {
    fn line_segment(&mut self, points: [Pos2; 2], stroke: Stroke) {
        neko_ui::Painter::line_segment(self, points, stroke);
    }
    fn circle_filled(&mut self, center: Pos2, radius: f32, color: Color32) {
        neko_ui::Painter::circle_filled(self, center, radius, color);
    }
    fn circle_stroke(&mut self, center: Pos2, radius: f32, stroke: Stroke) {
        neko_ui::Painter::circle_stroke(self, center, radius, stroke);
    }
    fn rect_filled(&mut self, rect: Rect, radius: CornerRadius, color: Color32) {
        neko_ui::Painter::rect_filled(self, rect, radius, color);
    }
    fn rect_stroke(&mut self, rect: Rect, radius: CornerRadius, stroke: Stroke, kind: StrokeKind) {
        neko_ui::Painter::rect_stroke(self, rect, radius, stroke, kind);
    }
    fn add(&mut self, shape: Shape) {
        neko_ui::Painter::add(self, shape);
    }
}


/// Draw `icon` centered in `rect` (designed for 14x14) on any canvas.
pub fn paint_on<C: Canvas + ?Sized>(p: &mut C, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 14.0;
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * s;
    let st = Stroke::new(1.4 * s.max(0.8), color);
    macro_rules! line {
        ($a:expr, $b:expr) => {
            p.line_segment([$a, $b], st)
        };
    }
    match icon {
        Icon::Plus => {
            line!(at(-5.0, 0.0), at(5.0, 0.0));
            line!(at(0.0, -5.0), at(0.0, 5.0));
        }
        Icon::Import | Icon::Download => {
            line!(at(0.0, -6.0), at(0.0, 2.0));
            line!(at(-3.5, -1.5), at(0.0, 2.0));
            line!(at(3.5, -1.5), at(0.0, 2.0));
            line!(at(-5.5, 3.0), at(-5.5, 5.5));
            line!(at(-5.5, 5.5), at(5.5, 5.5));
            line!(at(5.5, 5.5), at(5.5, 3.0));
        }
        Icon::Search => {
            p.circle_stroke(at(-1.5, -1.5), 4.0 * s, st);
            line!(at(1.5, 1.5), at(5.5, 5.5));
        }
        Icon::Folder => {
            p.add(Shape::convex_polygon(
                vec![at(-6.0, -4.5), at(-1.5, -4.5), at(0.0, -3.0), at(6.0, -3.0), at(6.0, 4.5), at(-6.0, 4.5)],
                color.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(-4.0, -5.0), at(5.0, 0.0), at(-4.0, 5.0)], color, Stroke::NONE));
        }
        Icon::Rename => {
            line!(at(0.0, -5.5), at(0.0, 5.5));
            line!(at(-2.5, -5.5), at(2.5, -5.5));
            line!(at(-2.5, 5.5), at(2.5, 5.5));
        }
        Icon::Duplicate => {
            p.rect_stroke(Rect::from_min_max(at(-2.0, -2.0), at(5.5, 5.5)), CornerRadius::same(1), st, StrokeKind::Middle);
            line!(at(-5.5, 2.5), at(-5.5, -5.5));
            line!(at(-5.5, -5.5), at(2.5, -5.5));
        }
        Icon::Tag => {
            p.add(Shape::closed_line(vec![at(-5.5, -5.5), at(0.5, -5.5), at(5.5, -0.5), at(-0.5, 5.5), at(-5.5, 0.5)], st));
            p.circle_filled(at(-2.5, -2.5), 1.1 * s, color);
        }
        Icon::Trash => {
            line!(at(-5.5, -3.5), at(5.5, -3.5));
            line!(at(-2.0, -5.5), at(2.0, -5.5));
            p.add(Shape::closed_line(vec![at(-4.0, -3.5), at(4.0, -3.5), at(3.0, 5.5), at(-3.0, 5.5)], st));
        }
        Icon::Broom => {
            line!(at(5.0, -5.5), at(-0.5, 0.5));
            p.add(Shape::convex_polygon(vec![at(-1.5, -0.5), at(1.0, 2.0), at(-2.5, 5.5), at(-5.5, 2.5)], color, Stroke::NONE));
        }
        Icon::Settings => {
            line!(at(-5.0, -3.0), at(5.0, -3.0));
            line!(at(-5.0, 3.0), at(5.0, 3.0));
            p.circle_filled(at(-1.5, -3.0), 1.8 * s, color);
            p.circle_filled(at(2.0, 3.0), 1.8 * s, color);
        }
        Icon::List => {
            for y in [-4.0, 0.0, 4.0] {
                p.rect_filled(Rect::from_center_size(at(-4.5, y), Vec2::splat(2.0 * s)), CornerRadius::ZERO, color);
                line!(at(-2.0, y), at(5.5, y));
            }
        }
        Icon::Ws2D => {
            line!(at(-5.0, 5.0), at(-5.0, -2.0));
            line!(at(-5.0, 5.0), at(2.0, 5.0));
            line!(at(-5.0, -2.0), at(-2.5, -4.5));
            line!(at(-5.0, -2.0), at(-7.0, -4.0));
            line!(at(2.0, 5.0), at(4.5, 2.5));
            line!(at(2.0, 5.0), at(4.5, 7.0));
        }
        Icon::Ws3D => {
            let pts = [at(0.0, -6.0), at(5.5, -3.0), at(5.5, 3.0), at(0.0, 6.0), at(-5.5, 3.0), at(-5.5, -3.0)];
            p.add(Shape::closed_line(pts.to_vec(), st));
            line!(at(-5.5, -3.0), at(0.0, 0.0));
            line!(at(5.5, -3.0), at(0.0, 0.0));
            line!(at(0.0, 0.0), at(0.0, 6.0));
        }
        Icon::Script => {
            p.add(Shape::closed_line(vec![at(-4.5, -6.0), at(4.5, -6.0), at(4.5, 6.0), at(-4.5, 6.0)], st));
            for y in [-3.0, 0.0, 3.0] {
                line!(at(-2.5, y), at(2.5, y));
            }
        }
        Icon::Game => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::new(13.0, 8.0) * s), CornerRadius::same(3), st, StrokeKind::Middle);
            line!(at(-4.5, 0.0), at(-1.5, 0.0));
            line!(at(-3.0, -1.5), at(-3.0, 1.5));
            p.circle_filled(at(3.0, -1.0), 0.9 * s, color);
            p.circle_filled(at(4.5, 1.0), 0.9 * s, color);
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(-4.0, -5.0), at(-1.0, 5.0)), CornerRadius::ZERO, color);
            p.rect_filled(Rect::from_min_max(at(1.0, -5.0), at(4.0, 5.0)), CornerRadius::ZERO, color);
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(9.0 * s)), CornerRadius::same(1), color);
        }
        Icon::PlayScene => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::new(13.0, 11.0) * s), CornerRadius::same(2), st, StrokeKind::Middle);
            p.add(Shape::convex_polygon(vec![at(-2.0, -3.0), at(3.5, 0.0), at(-2.0, 3.0)], color, Stroke::NONE));
        }
        Icon::Movie => {
            p.circle_stroke(c, 5.5 * s, st);
            p.circle_filled(c, 2.0 * s, color);
        }
        Icon::ToolSelect => {
            p.add(Shape::convex_polygon(vec![at(-4.0, -6.0), at(4.0, 2.0), at(0.0, 2.0), at(-4.0, 6.0)], color, Stroke::NONE));
        }
        Icon::ToolMove => {
            line!(at(-6.0, 0.0), at(6.0, 0.0));
            line!(at(0.0, -6.0), at(0.0, 6.0));
            for (a, b, c2) in [((-6.0, 0.0), (-4.0, -2.0), (-4.0, 2.0)), ((6.0, 0.0), (4.0, -2.0), (4.0, 2.0)), ((0.0, -6.0), (-2.0, -4.0), (2.0, -4.0)), ((0.0, 6.0), (-2.0, 4.0), (2.0, 4.0))] {
                p.add(Shape::convex_polygon(vec![at(a.0, a.1), at(b.0, b.1), at(c2.0, c2.1)], color, Stroke::NONE));
            }
        }
        Icon::ToolRotate => {
            let pts: Vec<Pos2> = (0..=20).map(|i| {
                let a = 0.4 + i as f32 / 20.0 * 5.0;
                at(a.cos() * 5.0, a.sin() * 5.0)
            }).collect();
            p.add(Shape::line(pts, st));
            p.add(Shape::convex_polygon(vec![at(5.5, -1.0), at(3.0, 1.0), at(6.5, 2.0)], color, Stroke::NONE));
        }
        Icon::ToolScale => {
            p.rect_stroke(Rect::from_min_max(at(-6.0, -1.0), at(1.0, 6.0)), CornerRadius::ZERO, st, StrokeKind::Middle);
            line!(at(-1.0, 1.0), at(6.0, -6.0));
            line!(at(6.0, -6.0), at(2.0, -6.0));
            line!(at(6.0, -6.0), at(6.0, -2.0));
        }
        Icon::Snap => {
            let pts: Vec<Pos2> = (0..=16).map(|i| {
                let a = std::f32::consts::PI * i as f32 / 16.0;
                at(-a.cos() * 4.5, a.sin() * 4.5 - 1.0)
            }).collect();
            p.add(Shape::line(pts, Stroke::new(2.4 * s, color)));
            line!(at(-4.5, -1.0), at(-4.5, -5.0));
            line!(at(4.5, -1.0), at(4.5, -5.0));
        }
        Icon::Grid => {
            for v in [-5.0, 0.0, 5.0] {
                line!(at(v, -6.0), at(v, 6.0));
                line!(at(-6.0, v), at(6.0, v));
            }
        }
        Icon::Close => {
            line!(at(-4.0, -4.0), at(4.0, 4.0));
            line!(at(4.0, -4.0), at(-4.0, 4.0));
        }
        Icon::Scene => {
            p.circle_stroke(at(0.0, -3.0), 2.5 * s, st);
            line!(at(0.0, -0.5), at(0.0, 2.5));
            line!(at(-4.5, 2.5), at(4.5, 2.5));
            line!(at(-4.5, 2.5), at(-4.5, 5.0));
            line!(at(4.5, 2.5), at(4.5, 5.0));
        }
        Icon::File => {
            p.add(Shape::closed_line(vec![at(-4.5, -6.0), at(2.0, -6.0), at(4.5, -3.5), at(4.5, 6.0), at(-4.5, 6.0)], st));
        }
        Icon::Star | Icon::StarFilled => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 7.0 } else { 3.0 };
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    c + Vec2::new(a.cos(), a.sin()) * r * s
                })
                .collect();
            if icon == Icon::StarFilled {
                for i in 0..10 {
                    p.add(Shape::convex_polygon(vec![c, pts[i], pts[(i + 1) % 10]], color, Stroke::NONE));
                }
            } else {
                p.add(Shape::closed_line(pts, Stroke::new(1.2, color)));
            }
        }
        Icon::Info => {
            p.circle_stroke(c, 6.0 * s, st);
            line!(at(0.0, -0.5), at(0.0, 3.5));
            p.circle_filled(at(0.0, -3.0), 0.9 * s, color);
        }
    }
}

/// The Neko cat head on any canvas.
pub fn cat_logo<C: Canvas + ?Sized>(p: &mut C, rect: Rect, accent: Color32, ink: Color32) {
    let s = rect.width() / 24.0;
    let pt = |x: f32, y: f32| rect.min + Vec2::new(x * s, y * s);
    p.rect_filled(Rect::from_min_max(pt(4.0, 8.0), pt(20.0, 22.0)), CornerRadius::same((6.0 * s) as u8), accent);
    p.add(Shape::convex_polygon(vec![pt(4.0, 3.0), pt(10.0, 9.0), pt(4.0, 11.0)], accent, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pt(20.0, 3.0), pt(20.0, 11.0), pt(14.0, 9.0)], accent, Stroke::NONE));
    p.circle_filled(pt(9.0, 14.0), 1.4 * s, ink);
    p.circle_filled(pt(15.0, 14.0), 1.4 * s, ink);
    p.add(Shape::convex_polygon(vec![pt(11.0, 16.6), pt(13.0, 16.6), pt(12.0, 17.6)], ink, Stroke::NONE));
}
