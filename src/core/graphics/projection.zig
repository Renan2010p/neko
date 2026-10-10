// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.projection` — cylindrical panorama projection with inverse mapping.
//!
//! `neko.render.cylinder` wraps a panorama on the GPU but hides the mapping, so
//! a game can use it for a backdrop and nothing else. This module keeps the
//! mapping instead: a flat `source_width`-wide texture is composited slice by
//! slice around the viewer, and a screen coordinate can be mapped back to a
//! panorama coordinate. That lets interactive hotspots live *inside* the
//! panorama — buttons that curve with the room and are still clickable.
//!
//! ```zig
//! var cyl = neko.projection.Cylinder.init(.{ .source_width = 1920 });
//! ...
//! cyl.composite(panorama_tex, pan);              // pan is -1..1
//! if (cyl.hit(pan, button, mouse_x, mouse_y)) {  // click through the curve
//!     ...
//! }
//! ```

const std: type = @import("std");
const math: type = std.math;
const types: type = @import("../base/types.zig");
const texture: type = @import("texture.zig");
const shader: type = @import("shader.zig");
const screen: type = @import("../system/screen.zig");

const PI: f32 = 3.14159265;
/// Upper bound on slices; the default (16px) fits a 4K screen comfortably.
const MAX_SLICES: usize = 512;

/// One screen column of the projection.
const Slice: type = struct {
    sx: i32,
    theta: f32,
    target_h: i32,
    src_w: i32,
};

/// Parameters for a `Cylinder`.
pub const Options: type = struct {
    /// Width of the panorama texture, in pixels.
    source_width: i32,
    /// Screen width to project onto (0 = the current logical size).
    screen_width: i32 = 0,
    /// Screen height to project onto (0 = the current logical size).
    screen_height: i32 = 0,
    /// Horizontal field of view, in degrees.
    fov_deg: f32 = 100,
    /// How far `pan` rotates the view, in degrees (half-range).
    pan_max_deg: f32 = 45,
    /// Column width, in pixels. Smaller = smoother, more slices.
    slice_width: i32 = 16,
    /// Column width for `composite_mesh`, in pixels (the shader-style path).
    mesh_step: i32 = 2,
};

/// Upper bound on the vertices `composite_mesh` can emit.
const MAX_MESH_COLS: usize = 2048;
var mesh_verts: [MAX_MESH_COLS * 2]types.Vertex = undefined;
var mesh_idx: [MAX_MESH_COLS * 6]i32 = undefined;

/// A reusable cylindrical projection of a flat panorama.
pub const Cylinder: type = struct {
    source_width: i32,
    screen_w: i32,
    screen_h: i32,
    focal: f32,
    pixels_per_radian: f32,
    pan_max: f32,
    slice_width: i32,
    mesh_step: i32,
    slices: [MAX_SLICES]Slice = undefined,
    count: usize = 0,

    /// Precomputes the projection for the given size.
    pub fn init(options: Options) Cylinder {
        const size: types.Point = screen.logical_size();
        const sw: i32 = if (options.screen_width > 0) options.screen_width else size.x;
        const sh: i32 = if (options.screen_height > 0) options.screen_height else size.y;

        var self: Cylinder = .{
            .source_width = options.source_width,
            .screen_w = sw,
            .screen_h = sh,
            .focal = 1,
            .pixels_per_radian = @as(f32, @floatFromInt(options.source_width)) / PI,
            .pan_max = options.pan_max_deg * PI / 180.0,
            .slice_width = @max(1, options.slice_width),
            .mesh_step = @max(1, options.mesh_step),
        };
        self.focal = (@as(f32, @floatFromInt(sw)) / 2.0) / @tan((options.fov_deg * PI / 180.0) / 2.0);
        self.build();
        return self;
    }

    fn build(self: *Cylinder) void {
        const slice_w: i32 = self.slice_width;
        self.count = 0;
        var sx: i32 = 0;
        while (sx < self.screen_w and self.count < MAX_SLICES) : (sx += slice_w) {
            const x: f32 = @as(f32, @floatFromInt(sx + @divTrunc(slice_w, 2))) - @as(f32, @floatFromInt(self.screen_w)) / 2.0;
            const theta: f32 = math.atan(x / self.focal);
            const c: f32 = @cos(theta);
            const scale_y: f32 = 1.0 / c;
            const src_w_d: f32 = @as(f32, @floatFromInt(slice_w)) * (self.pixels_per_radian * c * c / self.focal);
            self.slices[self.count] = Slice{
                .sx = sx,
                .theta = theta,
                .target_h = @intFromFloat(@as(f32, @floatFromInt(self.screen_h)) * scale_y),
                .src_w = @max(1, @as(i32, @intFromFloat(src_w_d))),
            };
            self.count += 1;
        }
    }

    /// The view angle for `pan` (-1..1, 0 = centred).
    fn center_angle(self: *const Cylinder, pan: f32) f32 {
        return PI / 2.0 + @max(-1.0, @min(1.0, pan)) * self.pan_max;
    }

    /// The panorama x a screen column sees, for hit-testing and placement.
    pub fn source_x(self: *const Cylinder, pan: f32, x: i32) f32 {
        const theta: f32 = math.atan((@as(f32, @floatFromInt(x)) - @as(f32, @floatFromInt(self.screen_w)) / 2.0) / self.focal);
        return (self.center_angle(pan) + theta) * self.pixels_per_radian;
    }

    /// The screen x a panorama coordinate lands on (inverse of `source_x`).
    pub fn screen_x(self: *const Cylinder, pan: f32, src: f32) f32 {
        const theta: f32 = src / self.pixels_per_radian - self.center_angle(pan);
        return @as(f32, @floatFromInt(self.screen_w)) / 2.0 + @tan(theta) * self.focal;
    }

    /// True when the screen point `(x, y)` is over `rect` in the panorama.
    pub fn hit(self: *const Cylinder, pan: f32, rect: types.Rect, x: i32, y: i32) bool {
        if (y < rect.y or y > rect.y + rect.h) return false;
        const sx: f32 = self.source_x(pan, x);
        return sx >= @as(f32, @floatFromInt(rect.x)) and sx <= @as(f32, @floatFromInt(rect.x + rect.w));
    }

    /// Composites `tex` (the panorama) to the current target/screen. `pan` is
    /// -1 (left end) .. 1 (right end); 0 centres it.
    pub fn composite(self: *const Cylinder, tex: types.TextureHandle, pan: f32) void {
        const center: f32 = self.center_angle(pan);
        const src_w_max: i32 = self.source_width;
        const scr_h: i32 = self.screen_h;

        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            const s = self.slices[i];
            const sx_f: f32 = (center + s.theta) * self.pixels_per_radian;
            const src_x: i32 = @intFromFloat(@floor(sx_f - @as(f32, @floatFromInt(s.src_w)) / 2.0));
            if (src_x + s.src_w <= 0 or src_x >= src_w_max) continue;

            const final_src_x: i32 = @max(0, src_x);
            const final_src_w: i32 = @min(src_w_max - final_src_x, s.src_w - (final_src_x - src_x));
            if (final_src_w <= 0) continue;

            texture.draw(
                tex,
                types.Rect{ .x = s.sx, .y = @divTrunc(scr_h - s.target_h, 2), .w = @max(1, self.slice_width + 1), .h = s.target_h },
                types.Rect{ .x = final_src_x, .y = 0, .w = final_src_w, .h = scr_h },
                null,
            );
        }
    }

    /// Composites `tex` as a dense quad strip with the panorama UV computed per
    /// column and shared at every boundary, in a single `geometry` call — the
    /// fragment-shader look, without shaders and without banding.
    pub fn composite_mesh(self: *const Cylinder, tex: types.TextureHandle, pan: f32) void {
        const w: i32 = self.screen_w;
        const h: i32 = self.screen_h;
        const center: f32 = self.center_angle(pan);
        const half: f32 = @as(f32, @floatFromInt(w)) / 2.0;
        const fh: f32 = @floatFromInt(h);
        const srcw: f32 = @floatFromInt(self.source_width);
        const step: i32 = @max(1, self.mesh_step);
        const white: types.Color = types.Color{ .r = 255, .g = 255, .b = 255 };

        var vn: usize = 0;
        var bc: usize = 0;
        var x: i32 = 0;
        while (x <= w and vn + 2 <= mesh_verts.len and bc + 1 < MAX_MESH_COLS) : (x += step) {
            const theta: f32 = math.atan((@as(f32, @floatFromInt(x)) - half) / self.focal);
            const target_h: f32 = fh / @cos(theta);
            const u: f32 = @max(0.0, @min(1.0, (center + theta) * self.pixels_per_radian / srcw));
            const y_top: f32 = (fh - target_h) / 2.0;
            const xf: f32 = @floatFromInt(x);
            mesh_verts[vn] = types.Vertex.init(xf, y_top, u, 0, white);
            mesh_verts[vn + 1] = types.Vertex.init(xf, y_top + target_h, u, 1, white);
            vn += 2;
            bc += 1;
        }

        var in: usize = 0;
        var q: usize = 0;
        while (q + 1 < bc) : (q += 1) {
            const base: i32 = @intCast(q * 2);
            mesh_idx[in] = base;
            mesh_idx[in + 1] = base + 1;
            mesh_idx[in + 2] = base + 2;
            mesh_idx[in + 3] = base + 1;
            mesh_idx[in + 4] = base + 3;
            mesh_idx[in + 5] = base + 2;
            in += 6;
        }
        if (in > 0) texture.geometry(tex, mesh_verts[0..vn], mesh_idx[0..in]);
    }

    /// Composites `tex` with a fragment shader when the backend supports
    /// shaders (`neko.shader`), returning true. Returns false otherwise, so the
    /// caller can fall back to `composite_mesh`. The GLSL computes the
    /// cylindrical UV per fragment — the Godot-shader look.
    pub fn composite_shader(self: *const Cylinder, tex: types.TextureHandle, pan: f32) bool {
        if (!shader.supported()) return false;
        if (cyl_program == null) {
            cyl_program = shader.load_builtin("cylinder") orelse shader.load(CYL_VS, CYL_FS);
        }
        const prog = cyl_program orelse return false;
        prog.draw(tex, .{
            pan,
            @floatFromInt(self.source_width),
            @floatFromInt(self.screen_w),
            @floatFromInt(self.screen_h),
        });
        return true;
    }
};

var cyl_program: ?shader.Program = null;

const CYL_VS =
    \\#version 330 core
    \\layout(location = 0) in vec2 a_pos;
    \\layout(location = 1) in vec2 a_uv;
    \\out vec2 v_screen;
    \\void main() {
    \\    v_screen = a_pos;
    \\    gl_Position = vec4(a_pos.x * 2.0 - 1.0, 1.0 - a_pos.y * 2.0, 0.0, 1.0);
    \\}
;

const CYL_FS =
    \\#version 330 core
    \\in vec2 v_screen;
    \\uniform sampler2D u_tex;
    \\uniform vec4 u_params; // pan, source_width, screen_w, screen_h
    \\out vec4 o_color;
    \\const float PI = 3.14159265;
    \\void main() {
    \\    float pan = u_params.x;
    \\    float srcw = u_params.y;
    \\    float sw = u_params.z;
    \\    float sh = u_params.w;
    \\    float focal = (sw * 0.5) / tan(radians(100.0) * 0.5);
    \\    float ppr = srcw / PI;
    \\    float center = PI * 0.5 + pan * radians(45.0);
    \\    float theta = atan(((v_screen.x - 0.5) * sw) / focal);
    \\    float u = (center + theta) * ppr / srcw;
    \\    float th = sh / cos(theta);
    \\    float y0 = (sh - th) * 0.5;
    \\    float sy = v_screen.y * sh;
    \\    if (sy < y0 || sy > y0 + th) { o_color = vec4(0.0); return; }
    \\    o_color = texture(u_tex, vec2(clamp(u, 0.0, 1.0), 1.0 - (sy - y0) / th));
    \\}
;

// ── Tests ────────────────────────────────────────────────────────────────────
const testing: type = @import("std").testing;

test "cylinder maps screen columns back to the panorama" {
    var cyl: Cylinder = .{
        .source_width = 1920,
        .screen_w = 1280,
        .screen_h = 720,
        .focal = (1280.0 / 2.0) / @tan((100.0 * 3.14159265 / 180.0) / 2.0),
        .pixels_per_radian = 1920.0 / 3.14159265,
        .pan_max = 45.0 * 3.14159265 / 180.0,
        .slice_width = 16,
        .mesh_step = 2,
    };
    cyl.build();

    // Centred: the middle column must map to the panorama centre.
    const mid: f32 = cyl.source_x(0, 640);
    try testing.expectApproxEqAbs(@as(f32, 960), mid, 1.0);

    // source_x and screen_x are inverses.
    const back: f32 = cyl.screen_x(0.3, cyl.source_x(0.3, 500));
    try testing.expectApproxEqAbs(@as(f32, 500), back, 0.5);
}
