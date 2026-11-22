//! Themes: colors, sizes and StyleBoxes per control state, like Godot's `Theme`.
//!
//! [`Theme::modern`] derives every color from a base color, an accent and a
//! contrast value with the same formulas as Godot 4's "Modern" editor theme
//! (`editor/themes/theme_modern.cpp`), so Neko's editor reads like Godot's.

use neko_render::epaint::{Color32, CornerRadius, Stroke};

use crate::geom::Sides;

/// A flat box: background, border and rounded corners, plus the padding it
/// adds around its content (Godot's `StyleBoxFlat` with content margins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleBox {
    pub bg: Color32,
    pub border: Stroke,
    pub radius: u8,
    pub padding: Sides,
}

impl StyleBox {
    pub const EMPTY: StyleBox = StyleBox { bg: Color32::TRANSPARENT, border: Stroke::NONE, radius: 0, padding: Sides::ZERO };

    pub const fn flat(bg: Color32, radius: u8, padding: Sides) -> Self {
        StyleBox { bg, border: Stroke::NONE, radius, padding }
    }

    pub fn with_border(mut self, width: f32, color: Color32) -> Self {
        self.border = Stroke::new(width, color);
        self
    }

    pub fn with_bg(mut self, bg: Color32) -> Self {
        self.bg = bg;
        self
    }

    pub fn with_padding(mut self, padding: Sides) -> Self {
        self.padding = padding;
        self
    }

    pub fn with_radius(mut self, r: u8) -> Self {
        self.radius = r;
        self
    }

    pub fn corner(&self) -> CornerRadius {
        CornerRadius::same(self.radius)
    }
}

/// Styles of a button-like control in each state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateStyles {
    pub normal: StyleBox,
    pub hover: StyleBox,
    pub pressed: StyleBox,
    pub disabled: StyleBox,
}

impl StateStyles {
    pub fn all(s: StyleBox) -> Self {
        Self { normal: s, hover: s, pressed: s, disabled: s }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub font_size: f32,
    pub font_color: Color32,
    pub font_hover_color: Color32,
    pub font_focus_color: Color32,
    pub font_disabled_color: Color32,
    pub font_muted_color: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// Window background (behind docks).
    pub background: Color32,
    /// CodeEdit background (Godot's `text_editor/theme/highlighting/background_color`).
    pub code_background: Color32,
    pub panel: StyleBox,
    /// Recessed areas: lists and trees.
    pub tree_panel: StyleBox,
    pub button: StateStyles,
    pub button_flat: StateStyles,
    pub button_primary: StateStyles,
    pub line_edit: StyleBox,
    pub line_edit_focus: StyleBox,
    pub check_box: Color32,
    pub check_box_border: Color32,
    pub dialog: StyleBox,
    pub popup_menu: StyleBox,
    pub popup_hover: Color32,
    pub tooltip: StyleBox,
    pub modal_dim: Color32,
    pub item_hover: Color32,
    pub item_selected: StyleBox,
    pub separator: Color32,
    pub separation: f32,
    pub corner_radius: u8,
}

/// sRGB color as floats, for Godot-style HSV adjustments.
#[derive(Clone, Copy)]
struct C(f32, f32, f32, f32);

impl C {
    fn from(c: Color32) -> Self {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        C(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
    }
    fn to(self) -> Color32 {
        let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(f(self.0), f(self.1), f(self.2), f(self.3))
    }
    fn lerp(self, o: C, t: f32) -> C {
        C(self.0 + (o.0 - self.0) * t, self.1 + (o.1 - self.1) * t, self.2 + (o.2 - self.2) * t, self.3 + (o.3 - self.3) * t)
    }
    fn hsv(self) -> (f32, f32, f32) {
        let (r, g, b) = (self.0, self.1, self.2);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };
        (h, if max == 0.0 { 0.0 } else { d / max }, max)
    }
    fn from_hsv(h: f32, s: f32, v: f32, a: f32) -> C {
        let i = (h * 6.0).floor();
        let f = h * 6.0 - i;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
        let (r, g, b) = match (i as i32).rem_euclid(6) {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        C(r, g, b, a)
    }
}

/// Godot's `_get_base_color`: darken (positive) or lighten (negative) the
/// base by the contrast, and scale its saturation.
fn base_color(base: Color32, contrast: f32, dimness: f32, saturation: f32) -> Color32 {
    let c = C::from(base);
    let contrast = if dimness < 0.0 { contrast.clamp(-0.1, 0.5) } else { contrast };
    let (h, s, v) = c.hsv();
    let v = (v + (0.0 - v) * contrast * dimness).clamp(0.0, 1.0);
    C::from_hsv(h, s * saturation, v, c.3).to()
}

fn alpha(c: Color32, a: f32) -> Color32 {
    let mut x = C::from(c);
    x.3 *= a;
    x.to()
}

impl Theme {
    /// Godot 4's "Default" preset (base 0.161 = `#292929`, contrast 0.3,
    /// see `editor_theme_manager.cpp`) with Neko's amber accent.
    pub fn neko_dark() -> Self {
        Self::modern(Color32::from_rgb(41, 41, 41), Color32::from_rgb(0xf2, 0xb5, 0x44), 0.3)
    }

    /// Build a theme the way Godot's Modern editor theme does.
    pub fn modern(base: Color32, accent: Color32, contrast: f32) -> Self {
        let b = |dim: f32, sat: f32| base_color(base, contrast, dim, sat);
        let white = Color32::WHITE;
        let radius = 4u8;
        let base_margin = 4.0;
        let increased_margin = 4.0;

        let surface_popup = b(1.9, 0.9);
        let surface_lowest = b(1.7, 0.9);
        let surface_lower = b(1.1, 0.9);
        let surface_base = base;
        let dark_color_1 = C::from(base).lerp(C(0.0, 0.0, 0.0, 1.0), contrast * 1.15).to();

        let button_normal = b(-2.0, 0.85);
        let button_hover = b(-2.9, 0.75);
        let button_pressed = b(-3.2, 0.75);
        let button_disabled = b(-1.4, 0.75);
        let border_normal = b(-2.5, 0.75);
        let border_hover = b(-3.4, 0.75);
        let border_pressed = b(-3.7, 0.75);
        let flat_hover = b(-1.2, 0.75);
        let flat_pressed = b(-2.0, 0.75);

        let button_pad = Sides::symmetric(base_margin * 2.0, base_margin * 1.5);
        let button = StyleBox::flat(button_normal, radius, button_pad).with_border(1.0, border_normal);
        let flat = StyleBox::flat(Color32::TRANSPARENT, radius, button_pad);
        let text_pad = Sides::symmetric(base_margin * 2.0, base_margin * 0.75 + 1.0);
        let line_edit = StyleBox::flat(surface_lower, radius, text_pad).with_border(1.0, alpha(white, 0.08));
        let accent_hover = C::from(accent).lerp(C(1.0, 1.0, 1.0, 1.0), 0.15).to();
        let accent_pressed = C::from(accent).lerp(C(0.0, 0.0, 0.0, 1.0), 0.12).to();
        let accent_text = Color32::from_rgb(0x1a, 0x14, 0x05);

        Self {
            font_size: 14.0,
            font_color: alpha(white, 0.75),
            font_hover_color: alpha(white, 0.85),
            font_focus_color: white,
            font_disabled_color: alpha(white, 0.35),
            font_muted_color: alpha(white, 0.5),
            accent,
            accent_text,
            background: dark_color_1,
            code_background: C::from(base).lerp(C(0.0, 0.0, 0.0, 1.0), contrast * 1.2).to(),
            panel: StyleBox::flat(surface_base, 0, Sides::ZERO),
            tree_panel: StyleBox::flat(surface_lowest, radius, Sides::all(increased_margin)),
            button: StateStyles {
                normal: button,
                hover: button.with_bg(button_hover).with_border(1.0, border_hover),
                pressed: button.with_bg(button_pressed).with_border(1.0, border_pressed),
                disabled: button.with_bg(button_disabled).with_border(1.0, alpha(white, 0.1)),
            },
            button_flat: StateStyles { normal: flat, hover: flat.with_bg(flat_hover), pressed: flat.with_bg(flat_pressed), disabled: flat },
            button_primary: StateStyles {
                normal: button.with_bg(accent).with_border(1.0, accent),
                hover: button.with_bg(accent_hover).with_border(1.0, accent_hover),
                pressed: button.with_bg(accent_pressed).with_border(1.0, accent_pressed),
                disabled: button.with_bg(alpha(accent, 0.3)).with_border(1.0, alpha(accent, 0.3)),
            },
            line_edit,
            line_edit_focus: line_edit.with_border(2.0, alpha(accent, 0.8)),
            check_box: surface_lower,
            check_box_border: alpha(white, 0.45),
            dialog: StyleBox::flat(surface_base, radius * 2, Sides::all(base_margin * 2.4)).with_border(1.0, border_normal),
            popup_menu: StyleBox::flat(surface_popup, radius, Sides::all(base_margin)).with_border(1.0, border_normal),
            popup_hover: b(-0.5, 0.75),
            tooltip: StyleBox::flat(surface_popup, radius, Sides::symmetric(8.0, 4.0)).with_border(1.0, border_normal),
            modal_dim: Color32::from_black_alpha(140),
            item_hover: alpha(white, 0.06),
            item_selected: StyleBox::flat(alpha(accent, 0.18), radius, Sides::ZERO).with_border(1.0, alpha(accent, 0.45)),
            separator: alpha(white, 0.1),
            separation: base_margin,
            corner_radius: radius,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godot_surface_values() {
        // Godot default: base v=0.14, contrast 0.3. surface_lower = lerp(0.14, 0, 0.33) = 0.0938.
        let base = Color32::from_rgb(36, 36, 36);
        let lower = base_color(base, 0.3, 1.1, 0.9);
        assert!((lower.r() as i32 - 24).abs() <= 1, "{lower:?}");
        // Buttons are lighter than the base (negative dimness).
        let button = base_color(base, 0.3, -2.0, 0.85);
        assert!(button.r() > base.r());
    }
}
