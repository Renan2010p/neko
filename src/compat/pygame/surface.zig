// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! A pygame-style CPU `Surface` (a `0xAARRGGBB` pixel buffer).

const std: type = @import("std");
const neko: type = @import("neko");
const pixel: type = @import("pixel.zig");
const rect_mod: type = @import("rect.zig");

const Rect: type = rect_mod.Rect;
const Point: type = rect_mod.Point;
const Color: type = neko.Color;
const Allocator: type = std.mem.Allocator;

/// A CPU pixel buffer, like a pygame `Surface`.  `pixels` is `0xAARRGGBB`.
pub const Surface: type = struct {
    w: u32,
    h: u32,
    pixels: []u32,
    alpha: u8 = 255,
    /// False when the surface has no per-pixel alpha (a plain, non-SRCALPHA
    /// surface): `blit` can then copy rows instead of blending.
    has_alpha: bool = true,
    allocator: Allocator,

    pub fn init(allocator: Allocator, w: u32, h: u32) !Surface {
        const px: []u32 = try allocator.alloc(u32, w * h);
        @memset(px, 0);
        return .{ .w = w, .h = h, .pixels = px, .allocator = allocator };
    }

    pub fn deinit(self: *Surface) void {
        self.allocator.free(self.pixels);
        self.pixels = &.{};
    }

    pub fn get_width(self: *const Surface) i32 {
        return @intCast(self.w);
    }
    pub fn get_height(self: *const Surface) i32 {
        return @intCast(self.h);
    }
    pub fn get_size(self: *const Surface) Point {
        return .{ .x = @intCast(self.w), .y = @intCast(self.h) };
    }
    pub fn get_rect(self: *const Surface) Rect {
        return Rect.init(0, 0, @intCast(self.w), @intCast(self.h));
    }

    pub fn set_alpha(self: *Surface, a: u8) void {
        self.alpha = a;
    }

    pub fn fill(self: *Surface, color: Color) void {
        @memset(self.pixels, pixel.pack(color));
    }

    /// Fills the given rectangle (clipped to the surface).
    pub fn fill_rect(self: *Surface, r: Rect, color: Color) void {
        const w: i32 = @intCast(self.w);
        const h: i32 = @intCast(self.h);
        const x0: i32 = @max(0, r.x);
        const y0: i32 = @max(0, r.y);
        const x1: i32 = @min(w, r.right());
        const y1: i32 = @min(h, r.bottom());
        if (x1 <= x0 or y1 <= y0) return;
        const v: u32 = pixel.pack(color);
        const ux0: usize = @intCast(x0);
        const ux1: usize = @intCast(x1);
        const uw: usize = @intCast(self.w);
        var y: i32 = y0;
        while (y < y1) : (y += 1) {
            const base: usize = @as(usize, @intCast(y)) * uw;
            @memset(self.pixels[base + ux0 .. base + ux1], v);
        }
    }

    pub fn copy(self: *const Surface) !Surface {
        const px: []u32 = try self.allocator.alloc(u32, self.w * self.h);
        @memcpy(px, self.pixels);
        return .{ .w = self.w, .h = self.h, .pixels = px, .allocator = self.allocator, .alpha = self.alpha, .has_alpha = self.has_alpha };
    }

    /// Alpha-composites `src` onto this surface at `dst` (pygame `blit`).
    pub fn blit(self: *Surface, src: *const Surface, dst: Point) void {
        const sw: i32 = @intCast(src.w);
        const sh: i32 = @intCast(src.h);
        const mx: i32 = @intCast(self.w);
        const my: i32 = @intCast(self.h);
        const sx0: i32 = @max(0, -dst.x);
        const sy0: i32 = @max(0, -dst.y);
        const sx1: i32 = @min(sw, mx - dst.x);
        const sy1: i32 = @min(sh, my - dst.y);
        if (sx1 <= sx0 or sy1 <= sy0) return;

        // Opaque source with no surface alpha: straight row copy (the common
        // "sprite has no transparency" case). This is what keeps many on-screen
        // sprites from slowing down.
        if (!src.has_alpha and src.alpha == 255) {
            const dstride: usize = self.w;
            const sstride: usize = src.w;
            const run: usize = @intCast(sx1 - sx0);
            const dx: usize = @intCast(dst.x + sx0);
            var y: i32 = sy0;
            while (y < sy1) : (y += 1) {
                const srow: usize = @as(usize, @intCast(y)) * sstride + @as(usize, @intCast(sx0));
                const drow: usize = @as(usize, @intCast(dst.y + y)) * dstride + dx;
                @memcpy(self.pixels[drow .. drow + run], src.pixels[srow .. srow + run]);
            }
            return;
        }

        // Translucent source: SIMD (AVX2-width) branch-free blend. This is the
        // hot path for many on-screen sprites and the reason Neko stays fast
        // where pygame's scalar blit slows down.
        const sw_usize: usize = src.w;
        const dw_usize: usize = self.w;
        const extra: u32 = src.alpha;
        var y: i32 = sy0;
        while (y < sy1) : (y += 1) {
            var di: usize = @as(usize, @intCast(dst.y + y)) * dw_usize + @as(usize, @intCast(dst.x + sx0));
            var si: usize = @as(usize, @intCast(y)) * sw_usize + @as(usize, @intCast(sx0));
            var x: i32 = sx0;
            while (x + pixel.LANES_I <= sx1) : (x += pixel.LANES_I) {
                const sp: pixel.V = src.pixels[si..][0..pixel.LANES].*;
                const dp: pixel.V = self.pixels[di..][0..pixel.LANES].*;
                self.pixels[di..][0..pixel.LANES].* = pixel.blendV(sp, dp, extra);
                di += pixel.LANES;
                si += pixel.LANES;
            }
            while (x < sx1) : (x += 1) {
                self.pixels[di] = pixel.blend1(src.pixels[si], self.pixels[di], extra);
                di += 1;
                si += 1;
            }
        }
    }

    pub fn set_pixel(self: *Surface, x: i32, y: i32, color: Color) void {
        self.put(x, y, color);
    }

    pub fn get_pixel(self: *const Surface, x: i32, y: i32) Color {
        if (x < 0 or y < 0 or x >= self.w or y >= self.h) return Color.rgb(0, 0, 0);
        return pixel.unpack(self.pixels[@intCast(@as(i32, @intCast(self.w)) * y + x)]);
    }

    /// Writes a pixel directly (alpha included, no blending). This is what
    /// `draw.*` and `fill` use; only `blit` composites.
    pub fn put(self: *Surface, x: i32, y: i32, color: Color) void {
        if (!self.in_bounds(x, y)) return;
        const i: usize = @intCast(@as(i32, @intCast(self.w)) * y + x);
        self.pixels[i] = pixel.pack(color);
    }

    fn in_bounds(self: *const Surface, x: i32, y: i32) bool {
        return x >= 0 and y >= 0 and x < self.w and y < self.h;
    }
};
