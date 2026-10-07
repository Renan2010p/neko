//! `engine.text` — font loading and text drawing.

const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

/// Optional parameters shared by the text functions (`neko.text` and
/// `neko.sprite`). Defaults mirror the C++ engine: size 24, white, not
/// centred, and the automatic font for the requested size.
pub const Options: type = struct {
    size: u32 = 24,
    color: types.Color = types.Color{ .r = 255, .g = 255, .b = 255 },
    center: bool = false,
    font: i64 = -1,
};

/// Loads a font at a fixed pixel size and returns its index.
pub fn load_font(path: []const u8, pixel_size: u16) i64 {
    const e: backend.Backend = context.get() orelse return -1;
    return e.load_font(path, pixel_size);
}

/// Draws `text` at (x, y). If the backend has no font, nothing is drawn.
pub fn draw(str: []const u8, x: i32, y: i32, options: Options) void {
    const e: backend.Backend = context.get() orelse return;
    _ = e.draw_text(str, x, y, options.size, options.color, options.center, options.font);
}

/// Draws `text` rotated by `angle` degrees around its top-left.
pub fn draw_rotated(str: []const u8, x: i32, y: i32, angle: f32, options: Options) void {
    const e: backend.Backend = context.get() orelse return;
    _ = e.draw_text_rotated(str, x, y, options.size, angle, options.color, options.center, options.font);
}

/// Measures `text` with the given font index, in pixels.
pub fn size(text: []const u8, font_idx: u32) ?types.Point {
    const e: backend.Backend = context.get() orelse return null;
    return e.text_size(text, font_idx);
}
