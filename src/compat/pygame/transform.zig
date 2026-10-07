// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `transform` — pygame-shaped nearest-neighbour scale and flip.

const surface_mod: type = @import("surface.zig");
const Surface: type = surface_mod.Surface;

pub const transform: type = struct {
    pub fn scale(src: *const Surface, w: u32, h: u32) !Surface {
        var out: Surface = try Surface.init(src.allocator, w, h);
        if (src.w == 0 or src.h == 0) return out;
        var y: u32 = 0;
        while (y < h) : (y += 1) {
            const sy: u32 = @min(@as(u32, @intCast(@as(u64, y) * src.h / h)), src.h - 1);
            var x: u32 = 0;
            while (x < w) : (x += 1) {
                const sx: u32 = @min(@as(u32, @intCast(@as(u64, x) * src.w / w)), src.w - 1);
                out.pixels[y * w + x] = src.pixels[sy * src.w + sx];
            }
        }
        return out;
    }

    pub fn flip_x(src: *const Surface) !Surface {
        var out: Surface = try Surface.init(src.allocator, src.w, src.h);
        var y: u32 = 0;
        while (y < src.h) : (y += 1) {
            var x: u32 = 0;
            while (x < src.w) : (x += 1) {
                out.pixels[y * src.w + x] = src.pixels[y * src.w + (src.w - 1 - x)];
            }
        }
        return out;
    }

    /// Takes a `flip_x` surface and returns the one to draw, or the original.
    pub fn maybeFlipX(src: *const Surface, flip_h: bool, holder: *?Surface) *const Surface {
        if (!flip_h) return src;
        if (holder.* == null) {
            holder.* = flip_x(src) catch return src;
        }
        return &holder.*.?;
    }
};
