//! `neko.render` — reusable rendering presets built on the engine primitives.
//!
//! These are "styles" a game can apply to a plain texture. `cylinder` wraps a
//! flat panorama texture around the viewer, compositing it on the GPU — no
//! per-slice CPU draws.

const std: type = @import("std");
const types: type = @import("../base/types.zig");
const texture: type = @import("texture.zig");
const screen: type = @import("../system/screen.zig");

const PI: f32 = 3.14159265;
const MAX_COLUMNS: usize = 256;

/// Parameters for `cylinder`.
pub const CylinderOptions: type = struct {
    /// Width of the panorama texture in pixels.
    source_width: i32,
    /// Horizontal pan, -1..1.
    pan: f32 = 0,
    /// Horizontal field of view, in degrees.
    fov_deg: f32 = 100,
    /// How far pan rotates the view, in degrees (half-range).
    pan_max_deg: f32 = 45,
    /// Column width in pixels. Smaller = smoother curve, more triangles.
    slice_width: i32 = 16,
};

var verts: [MAX_COLUMNS * 4]types.Vertex = undefined;
var idx: [MAX_COLUMNS * 6]i32 = undefined;

/// Draws `tex` (a flat panorama) as if wrapped on a cylinder around the viewer,
/// filling the current logical screen. Each column is scaled vertically by
/// `1/cos(theta)` and samples a `cos^2(theta)` wide band of the panorama.
pub fn cylinder(tex: types.TextureHandle, options: CylinderOptions) void {
    const size: types.Point = screen.logical_size();
    const screen_w: f32 = @floatFromInt(size.x);
    const screen_h: f32 = @floatFromInt(size.y);
    if (screen_w <= 0 or screen_h <= 0 or options.source_width <= 0) return;

    const fov: f32 = options.fov_deg * PI / 180.0;
    const focal: f32 = (screen_w / 2.0) / @tan(fov / 2.0);
    const ppr: f32 = @as(f32, @floatFromInt(options.source_width)) / PI;
    const slice_w: f32 = @floatFromInt(options.slice_width);
    const max_pan: f32 = options.pan_max_deg * PI / 180.0;
    const center_angle: f32 = PI / 2.0 + options.pan * max_pan;
    const panorama_w: f32 = @floatFromInt(options.source_width);
    const white: types.Color = types.Color{ .r = 255, .g = 255, .b = 255 };

    var vn: usize = 0;
    var in: usize = 0;

    var sx: f32 = 0;
    while (sx < screen_w and vn + 4 <= verts.len) : (sx += slice_w) {
        const theta: f32 = std.math.atan((sx + slice_w / 2.0 - screen_w / 2.0) / focal);
        const cos_t: f32 = @cos(theta);
        const target_h: f32 = screen_h / cos_t;
        const src_w: i32 = @max(1, @as(i32, @intFromFloat(slice_w * (ppr * cos_t * cos_t / focal))));

        const source_x: f32 = (center_angle + theta) * ppr;
        const src_x: i32 = @intFromFloat(std.math.floor(source_x - @as(f32, @floatFromInt(src_w)) / 2.0));
        if (src_x + src_w <= 0 or src_x >= options.source_width) continue;

        const final_src_x: i32 = @max(0, src_x);
        const final_src_w: i32 = @min(options.source_width - final_src_x, src_w - (final_src_x - src_x));
        if (final_src_w <= 0) continue;

        const ul: f32 = @as(f32, @floatFromInt(final_src_x)) / panorama_w;
        const ur: f32 = @as(f32, @floatFromInt(final_src_x + final_src_w)) / panorama_w;
        const x0: f32 = sx;
        const x1: f32 = @min(sx + slice_w + 1.0, screen_w);
        const y_top: f32 = (screen_h - target_h) / 2.0;
        const y_bot: f32 = y_top + target_h;

        verts[vn + 0] = types.Vertex.init(x0, y_top, ul, 0, white);
        verts[vn + 1] = types.Vertex.init(x1, y_top, ur, 0, white);
        verts[vn + 2] = types.Vertex.init(x0, y_bot, ul, 1, white);
        verts[vn + 3] = types.Vertex.init(x1, y_bot, ur, 1, white);

        const ib: i32 = @intCast(vn);
        idx[in + 0] = ib + 0;
        idx[in + 1] = ib + 1;
        idx[in + 2] = ib + 2;
        idx[in + 3] = ib + 1;
        idx[in + 4] = ib + 3;
        idx[in + 5] = ib + 2;

        vn += 4;
        in += 6;
    }

    if (in > 0) texture.geometry(tex, verts[0..vn], idx[0..in]);
}
