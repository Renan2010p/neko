//! `engine.draw` — drawing primitives.
//!
//!     engine.draw.clear(engine.Color{ .r = 3, .g = 3, .b = 5 });
//!     engine.draw.rect(.{ .x = 10, .y = 10, .w = 100, .h = 40 }, color, true);

const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");
const sprite_mod: type = @import("sprite.zig");

/// Fills the whole frame buffer with `color`.
pub fn clear(color: types.Color) void {
    const e: backend.Backend = context.get() orelse return;
    e.clear(color);
}

/// Fills (or outlines) an axis-aligned rectangle.
pub fn rect(r: types.Rect, color: types.Color, filled: bool) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_rect(r, color, filled);
}

/// Draws a line from (x1, y1) to (x2, y2).
pub fn line(x1: i32, y1: i32, x2: i32, y2: i32, color: types.Color) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_line(x1, y1, x2, y2, color);
}

/// Draws a circle centred at (cx, cy).
pub fn circle(cx: i32, cy: i32, radius: i32, color: types.Color, filled: bool) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_circle(cx, cy, radius, color, filled);
}

/// Fills the convex quad p1→p2→p3→p4 using the platform's default scanline
/// fill (the same algorithm the C++ `Engine::fill_quad` default uses).
pub fn quad(p1: types.Point, p2: types.Point, p3: types.Point, p4: types.Point, color: types.Color) void {
    const y0: i32 = @min(p1.y, p2.y);
    const y1: i32 = @max(p3.y, p4.y);
    if (y1 <= y0) return;

    const span: f32 = @floatFromInt(y1 - y0);
    var py: i32 = y0;
    while (py <= y1) : (py += 1) {
        const t: f32 = @as(f32, @floatFromInt(py - y0)) / span;
        const xl: i32 = @intFromFloat(@as(f32, @floatFromInt(p1.x)) + @as(f32, @floatFromInt(p4.x - p1.x)) * t);
        const xr: i32 = @intFromFloat(@as(f32, @floatFromInt(p2.x)) + @as(f32, @floatFromInt(p3.x - p2.x)) * t);
        if (xr > xl) line(xl, py, xr, py, color);
    }
}

/// Draws a small filled star centred at (cx, cy). Composed of two rectangles,
/// exactly like the C++ `DrawUtils::star`.
pub fn star(cx: i32, cy: i32, outer_r: i32, color: types.Color) void {
    const size: i32 = outer_r;
    rect(.{ .x = cx - @divTrunc(size, 4), .y = cy - @divTrunc(size, 2), .w = @divTrunc(size, 2), .h = size }, color, true);
    rect(.{ .x = cx - @divTrunc(size, 2), .y = cy - @divTrunc(size, 4), .w = size, .h = @divTrunc(size, 2) }, color, true);
}

/// Draws text with the cached font (picked by size). Same as `neko.sprite.text`.
pub fn text(str: []const u8, x: i32, y: i32, options: sprite_mod.Options) void {
    sprite_mod.text(str, x, y, options);
}

/// Draws a named sprite. Same as `neko.sprite.draw`.
pub fn sprite(name: []const u8, x: i32, y: i32, w: i32, h: i32) void {
    sprite_mod.draw(name, x, y, w, h);
}
