//! The boot splash: logo, name and subtitle over a solid color, like Godot's
//! `application/boot_splash` (shown by the editor and by games while they load).

use neko_render::epaint::{Color32, CornerRadius, FontId, Shape, Stroke};

use crate::control::{Anchors, Control};
use crate::geom::{Pos2, Rect, Vec2, pos2, vec2};
use crate::paint::Painter;

/// Godot's default `boot_splash/bg_color` (0.14, 0.14, 0.14).
pub const SPLASH_BG: Color32 = Color32::from_rgb(36, 36, 36);
pub const NEKO_AMBER: Color32 = Color32::from_rgb(0xf2, 0xb5, 0x44);
const INK: Color32 = Color32::from_rgb(0x2b, 0x22, 0x14);

/// Draw the Neko cat head filling `rect` (square works best), with a white
/// outline like Godot's logo.
pub fn draw_logo(p: &mut Painter, rect: Rect) {
    let s = rect.width().min(rect.height()) / 100.0;
    let o = rect.center() - vec2(50.0, 50.0) * s;
    let pt = |x: f32, y: f32| o + vec2(x, y) * s;
    let outline = 4.0 * s;
    let head = Rect::from_min_max(pt(14.0, 30.0), pt(86.0, 90.0));
    let ear_l = [pt(14.0, 52.0), pt(16.0, 8.0), pt(44.0, 31.0)];
    let ear_r = [pt(86.0, 52.0), pt(84.0, 8.0), pt(56.0, 31.0)];
    let radius = CornerRadius::same((26.0 * s).min(255.0) as u8);

    // White silhouette slightly larger, then the amber shape on top.
    for ear in [ear_l, ear_r] {
        p.add(Shape::convex_polygon(ear.to_vec(), Color32::WHITE, Stroke::new(outline * 2.0, Color32::WHITE)));
    }
    p.rect_filled(head.expand(outline), CornerRadius::same((26.0 * s + outline).min(255.0) as u8), Color32::WHITE);
    for ear in [ear_l, ear_r] {
        p.add(Shape::convex_polygon(ear.to_vec(), NEKO_AMBER, Stroke::NONE));
    }
    p.rect_filled(head, radius, NEKO_AMBER);
    // Inner ears.
    let inner = Color32::from_rgb(0xe0, 0x96, 0x2e);
    p.add(Shape::convex_polygon(vec![pt(22.0, 40.0), pt(23.0, 20.0), pt(36.0, 32.0)], inner, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pt(78.0, 40.0), pt(77.0, 20.0), pt(64.0, 32.0)], inner, Stroke::NONE));
    // Eyes: white with dark pupils, like Godot's robot.
    for x in [35.0, 65.0] {
        p.circle_filled(pt(x, 56.0), 9.0 * s, Color32::WHITE);
        p.circle_filled(pt(x + 1.0, 57.0), 5.0 * s, INK);
    }
    // Nose and mouth.
    p.add(Shape::convex_polygon(vec![pt(46.0, 67.0), pt(54.0, 67.0), pt(50.0, 72.0)], INK, Stroke::NONE));
    let mouth = Stroke::new(2.4 * s, INK);
    p.add(Shape::line(vec![pt(50.0, 72.0), pt(46.0, 77.0), pt(41.0, 76.0)], mouth));
    p.add(Shape::line(vec![pt(50.0, 72.0), pt(54.0, 77.0), pt(59.0, 76.0)], mouth));
    // Whiskers.
    let wh = Stroke::new(1.8 * s, INK.gamma_multiply(0.7));
    for (a, b) in [((26.0, 68.0), (6.0, 64.0)), ((26.0, 72.0), (7.0, 75.0)), ((74.0, 68.0), (94.0, 64.0)), ((74.0, 72.0), (93.0, 75.0))] {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], wh);
    }
}

/// The title in Inter Bold (the bundled bold face).
fn bold_text(p: &mut Painter, center: Pos2, text: &str, size: f32, color: Color32) -> Rect {
    let galley = p.text_server().layout_bold(text, size, color);
    let r = Rect::from_center_size(center, galley.size());
    p.galley(r.min, galley, color);
    r
}

/// Draw the whole splash into `rect`.
pub fn draw_splash(p: &mut Painter, rect: Rect, bg: Color32, show_image: bool, subtitle: &str) {
    p.rect_filled(rect, 0, bg);
    if !show_image {
        return;
    }
    // Scale down on small windows, never up (Godot's editor splash is not stretched).
    let scale = (rect.height() / 640.0).min(rect.width() / 700.0).clamp(0.45, 1.0);
    let logo = 150.0 * scale;
    let title = 92.0 * scale;
    let sub = 32.0 * scale;
    let total = logo + 18.0 * scale + title + sub;
    let top = rect.center().y - total / 2.0;
    draw_logo(p, Rect::from_center_size(pos2(rect.center().x, top + logo / 2.0), Vec2::splat(logo)));
    let title_y = top + logo + 18.0 * scale + title / 2.0;
    let t = bold_text(p, pos2(rect.center().x, title_y), "NEKO", title, Color32::WHITE);
    let gray = Color32::from_gray(150);
    p.text(pos2(rect.center().x, t.bottom() + 2.0 * scale), crate::Align2::CENTER_TOP, subtitle, FontId::proportional(sub), gray);
}

/// A full-window control showing the splash.
pub fn splash_control(bg: Color32, show_image: bool, subtitle: &'static str) -> Control {
    Control::custom(Vec2::ZERO, move |p, r, _, _| draw_splash(p, r, bg, show_image, subtitle)).anchors(Anchors::FULL_RECT)
}
