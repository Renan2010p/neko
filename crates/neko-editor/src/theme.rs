//! Editor colors, matching Godot 4.7's default dark theme (`editor/themes`,
//! "Default" preset: base 0.161, contrast 0.3). The only deliberate difference
//! from Godot is the accent: Neko amber `#f2b544` instead of Godot blue.
//!
//! Widget styles live in `neko_ui::Theme::neko_dark()`; these constants are for
//! things the editor draws itself (viewports, badges, tags).

use neko_ui::Color32;

pub const BG: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const FG: Color32 = Color32::from_rgb(0xca, 0xca, 0xca);
pub const MUTED: Color32 = Color32::from_rgb(0x9f, 0x9f, 0x9f);
pub const ACCENT: Color32 = Color32::from_rgb(0xf2, 0xb5, 0x44);
pub const OK: Color32 = Color32::from_rgb(0x73, 0xf2, 0x80);
pub const WARN: Color32 = Color32::from_rgb(0xd4, 0xc7, 0x9e);
pub const BAD: Color32 = Color32::from_rgb(0xff, 0x78, 0x6b);
pub const C2D: Color32 = Color32::from_rgb(0x8d, 0xa5, 0xf3);
pub const C3D: Color32 = Color32::from_rgb(0xfc, 0x7f, 0x7f);
pub const CUI: Color32 = Color32::from_rgb(0x8e, 0xef, 0x97);
pub const AXIS_X: Color32 = Color32::from_rgb(0xf5, 0x33, 0x52);
pub const AXIS_Y: Color32 = Color32::from_rgb(0x87, 0xd6, 0x03);
pub const AXIS_Z: Color32 = Color32::from_rgb(0x2a, 0x8c, 0xf5);
/// The 2D canvas background (`editors/2d/bg_color`, 0.3 gray).
pub const VIEWPORT: Color32 = Color32::from_rgb(0x4c, 0x4c, 0x4c);

pub fn tag_color(tag: &str) -> Color32 {
    match tag {
        "2d" => C2D,
        "3d" => C3D,
        "ui" => CUI,
        _ => MUTED,
    }
}

/// Stable color for a project's initials badge.
pub fn badge_color(name: &str) -> Color32 {
    const HUES: [Color32; 6] = [ACCENT, C2D, C3D, CUI, Color32::from_rgb(0xc7, 0x9b, 0xff), Color32::from_rgb(0x7f, 0xd3, 0xe0)];
    let sum: u32 = name.chars().map(|c| c as u32).sum();
    HUES[sum as usize % HUES.len()]
}
