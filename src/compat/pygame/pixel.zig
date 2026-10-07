// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Pixel math for the pygame layer: packing, and the alpha-compositing blend
//! (scalar and SIMD).
//!
//! Pixels are packed `0xAARRGGBB`, matching Neko's streaming texture format.

const std: type = @import("std");
const neko: type = @import("neko");

/// A floating-point vertex (for sub-pixel polygon fills).
pub const PointF: type = struct {
    x: f32 = 0,
    y: f32 = 0,
};

pub fn pack(c: neko.Color) u32 {
    return (@as(u32, c.a) << 24) | (@as(u32, c.r) << 16) | (@as(u32, c.g) << 8) | @as(u32, c.b);
}

pub fn unpack(p: u32) neko.Color {
    return .{ .r = @truncate(p >> 16), .g = @truncate(p >> 8), .b = @truncate(p), .a = @truncate(p >> 24) };
}

/// Exact `x / 255` for `0 <= x <= 255*255` using shifts (much cheaper than a
/// hardware division, and this runs once per pixel per channel on every blit).
pub inline fn div255(x: u32) u32 {
    const t = x + 128;
    return (t + (t >> 8)) >> 8;
}

// ── SIMD blend ───────────────────────────────────────────────────────────────

/// 8 pixels per step (256-bit, AVX2 on the native target).
pub const LANES: usize = 8;
pub const LANES_I: i32 = 8;
pub const V: type = @Vector(LANES, u32);
pub const Sh: type = @Vector(LANES, u5);

pub inline fn div255V(x: V) V {
    const t = x + @as(V, @splat(128));
    return (t + (t >> @as(Sh, @splat(8)))) >> @as(Sh, @splat(8));
}

/// Blends 8 packed `0xAARRGGBB` pixels at once (source-over `extra` alpha).
pub inline fn blendV(sp: V, dp: V, extra: u32) V {
    const s24: Sh = @splat(24);
    const s16: Sh = @splat(16);
    const s8: Sh = @splat(8);
    const mask: V = @splat(0xff);
    const sa = div255V((sp >> s24) * @as(V, @splat(extra)));
    const inv = @as(V, @splat(255)) - sa;
    const r = div255V(((sp >> s16) & mask) * sa + ((dp >> s16) & mask) * inv);
    const g = div255V(((sp >> s8) & mask) * sa + ((dp >> s8) & mask) * inv);
    const b = div255V((sp & mask) * sa + (dp & mask) * inv);
    const a = sa + div255V(((dp >> s24) & mask) * inv);
    return (a << s24) | (r << s16) | (g << s8) | b;
}

/// One pixel, same maths as `blendV`.
pub inline fn blend1(sp: u32, dp: u32, extra: u32) u32 {
    const sa = div255((sp >> 24) * extra);
    const inv = 255 - sa;
    const r = div255(((sp >> 16) & 0xff) * sa + ((dp >> 16) & 0xff) * inv);
    const g = div255(((sp >> 8) & 0xff) * sa + ((dp >> 8) & 0xff) * inv);
    const b = div255((sp & 0xff) * sa + (dp & 0xff) * inv);
    const a = sa + div255(((dp >> 24) & 0xff) * inv);
    return (a << 24) | (r << 16) | (g << 8) | b;
}
