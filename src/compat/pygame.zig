// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko_pygame` — a small, pygame-shaped compatibility layer on top of Neko.
//!
//! It gives a game the pygame model the Vesper prototype was written against:
//! a CPU `Surface` you draw into with primitives, alpha blits, `transform`
//! scale/flip, and a `display` that presents one canvas to the window.
//!
//! It only uses Neko's *public* API (textures + screen), so it does not touch
//! the engine core or the backends:
//!
//! ```zig
//! const pg = @import("neko_pygame");
//!
//! pub fn main(init: std.process.Init) !void {
//!     const screen = pg.set_mode(512, 288);
//!     try neko.run(init, .{ .width = 1024, .height = 576, .clear_color = null }, frame);
//! }
//!
//! fn frame(dt: f32) void {
//!     const s = pg.surface();
//!     s.fill(pg.Color.hex(0x101018));
//!     pg.draw.circle(s, 120, 90, 40, pg.Color.hex(0x4fd97a), true);
//!     pg.flip();
//! }
//! ```
//!
//! Pixels are packed `0xAARRGGBB` (matching Neko's streaming texture format).

const std: type = @import("std");
const neko: type = @import("neko");

const Allocator: type = std.mem.Allocator;

/// Alias of `neko.Color` (RGBA).  Use `Color.hex(0xRRGGBB)` like pygame.
pub const Color: type = neko.Color;
pub const Point: type = neko.Point;

/// A floating-point vertex (for `polygon_f`, which needs sub-pixel precision).
pub const PointF: type = struct {
    x: f32 = 0,
    y: f32 = 0,
};

// ── Rect ─────────────────────────────────────────────────────────────────────

/// A pygame-style rectangle with the helpers the game code reaches for.
pub const Rect: type = struct {
    x: i32 = 0,
    y: i32 = 0,
    w: i32 = 0,
    h: i32 = 0,

    pub fn init(x: i32, y: i32, w: i32, h: i32) Rect {
        return .{ .x = x, .y = y, .w = w, .h = h };
    }

    pub fn right(self: Rect) i32 {
        return self.x + self.w;
    }
    pub fn bottom(self: Rect) i32 {
        return self.y + self.h;
    }
    pub fn centerx(self: Rect) i32 {
        return self.x + @divTrunc(self.w, 2);
    }
    pub fn centery(self: Rect) i32 {
        return self.y + @divTrunc(self.h, 2);
    }
    pub fn center(self: Rect) Point {
        return .{ .x = self.centerx(), .y = self.centery() };
    }
    pub fn size(self: Rect) Point {
        return .{ .x = self.w, .y = self.h };
    }

    pub fn colliderect(self: Rect, other: Rect) bool {
        return self.x < other.right() and self.right() > other.x and
            self.y < other.bottom() and self.bottom() > other.y;
    }

    pub fn collidepoint(self: Rect, p: Point) bool {
        return p.x >= self.x and p.x < self.right() and p.y >= self.y and p.y < self.bottom();
    }

    /// Grows (positive) or shrinks (negative) the rect by `dw`/`dh` on both axes.
    pub fn inflate(self: Rect, dw: i32, dh: i32) Rect {
        return .{ .x = self.x - @divTrunc(dw, 2), .y = self.y - @divTrunc(dh, 2), .w = self.w + dw, .h = self.h + dh };
    }

    pub fn toNeko(self: Rect) neko.Rect {
        return neko.Rect.init(self.x, self.y, self.w, self.h);
    }
};

// ── Surface ──────────────────────────────────────────────────────────────────

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
        @memset(self.pixels, pack(color));
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
        const v: u32 = pack(color);
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
            while (x + LANES_I <= sx1) : (x += LANES_I) {
                const sp: V = src.pixels[si..][0..LANES].*;
                const dp: V = self.pixels[di..][0..LANES].*;
                self.pixels[di..][0..LANES].* = blendV(sp, dp, extra);
                di += LANES;
                si += LANES;
            }
            while (x < sx1) : (x += 1) {
                const sp = src.pixels[si];
                const dp = self.pixels[di];
                const sa = div255((sp >> 24) * extra);
                const inv = 255 - sa;
                const r = div255(((sp >> 16) & 0xff) * sa + ((dp >> 16) & 0xff) * inv);
                const g = div255(((sp >> 8) & 0xff) * sa + ((dp >> 8) & 0xff) * inv);
                const b = div255((sp & 0xff) * sa + (dp & 0xff) * inv);
                const a = sa + div255(((dp >> 24) & 0xff) * inv);
                self.pixels[di] = (a << 24) | (r << 16) | (g << 8) | b;
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
        return unpack(self.pixels[@intCast(@as(i32, @intCast(self.w)) * y + x)]);
    }

    // -- internals --------------------------------------------------------
    fn in_bounds(self: *const Surface, x: i32, y: i32) bool {
        return x >= 0 and y >= 0 and x < self.w and y < self.h;
    }

    fn put(self: *Surface, x: i32, y: i32, color: Color) void {
        // pygame.draw.* and fill() write the pixel directly (alpha included);
        // they do not alpha-blend. Only blit() composites.
        if (!self.in_bounds(x, y)) return;
        const i: usize = @intCast(@as(i32, @intCast(self.w)) * y + x);
        self.pixels[i] = pack(color);
    }

    fn blend_px(self: *Surface, x: i32, y: i32, src: u32, extra: u8) void {
        if (!self.in_bounds(x, y)) return;
        const i: usize = @intCast(@as(i32, @intCast(self.w)) * y + x);
        const dst: u32 = self.pixels[i];
        const out: u32 = blend(dst, src, extra);
        // keep the destination alpha sensible for later blits
        self.pixels[i] = out | (@as(u32, @max(src >> 24, dst >> 24)) << 24);
    }
};

// ── Pixel helpers ────────────────────────────────────────────────────────────

fn pack(c: Color) u32 {
    return (@as(u32, c.a) << 24) | (@as(u32, c.r) << 16) | (@as(u32, c.g) << 8) | @as(u32, c.b);
}

fn unpack(p: u32) Color {
    return .{ .r = @truncate(p >> 16), .g = @truncate(p >> 8), .b = @truncate(p), .a = @truncate(p >> 24) };
}

fn blend(dst: u32, src: u32, extra: u8) u32 {
    var sa: u32 = src >> 24;
    if (extra != 255) sa = div255(sa * @as(u32, extra));
    if (sa >= 255) return src | 0xff000000;
    if (sa == 0) return dst;
    const inv: u32 = 255 - sa;
    const r: u32 = div255(((src >> 16) & 0xff) * sa + ((dst >> 16) & 0xff) * inv);
    const g: u32 = div255(((src >> 8) & 0xff) * sa + ((dst >> 8) & 0xff) * inv);
    const b: u32 = div255((src & 0xff) * sa + (dst & 0xff) * inv);
    const a: u32 = sa + div255(((dst >> 24) & 0xff) * inv);
    return (a << 24) | (r << 16) | (g << 8) | b;
}

/// Exact `x / 255` for `0 <= x <= 255*255` using shifts (much cheaper than a
/// hardware division, and this runs once per pixel per channel on every blit).
inline fn div255(x: u32) u32 {
    const t = x + 128;
    return (t + (t >> 8)) >> 8;
}

// ── SIMD blend ───────────────────────────────────────────────────────────────

/// 8 pixels per step (256-bit, AVX2 on the native target).
const LANES: usize = 8;
const LANES_I: i32 = 8;
const V = @Vector(LANES, u32);
const Sh = @Vector(LANES, u5);

inline fn div255V(x: V) V {
    const t = x + @as(V, @splat(128));
    return (t + (t >> @as(Sh, @splat(8)))) >> @as(Sh, @splat(8));
}

/// Blends 8 packed `0xAARRGGBB` pixels at once, same maths as `blend`.
inline fn blendV(sp: V, dp: V, extra: u32) V {
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

// ── draw ─────────────────────────────────────────────────────────────────────

/// pygame.draw: primitives that render *into* a `Surface`.
pub const draw: type = struct {
    pub fn rect(s: *Surface, r: Rect, color: Color, filled: bool) void {
        if (filled) {
            s.fill_rect(r, color);
            return;
        }
        hline(s, r.x, r.right() - 1, r.y, color);
        hline(s, r.x, r.right() - 1, r.bottom() - 1, color);
        vline(s, r.x, r.y, r.bottom() - 1, color);
        vline(s, r.right() - 1, r.y, r.bottom() - 1, color);
    }

    /// Rectangle with rounded corners (`border_radius`), filled or an outline
    /// `thickness` pixels wide.
    pub fn round_rect(s: *Surface, r: Rect, color: Color, filled: bool, radius: i32, thickness: i32) void {
        const rr: i32 = @max(0, @min(radius, @min(@divTrunc(r.w, 2), @divTrunc(r.h, 2))));
        if (rr == 0) {
            const t: i32 = @max(1, thickness);
            if (filled) {
                s.fill_rect(r, color);
                return;
            }
            var i: i32 = 0;
            while (i < t) : (i += 1) {
                const ir: Rect = Rect.init(r.x + i, r.y + i, @max(1, r.w - 2 * i), @max(1, r.h - 2 * i));
                hline(s, ir.x, ir.right() - 1, ir.y, color);
                hline(s, ir.x, ir.right() - 1, ir.bottom() - 1, color);
                vline(s, ir.x, ir.y, ir.bottom() - 1, color);
                vline(s, ir.right() - 1, ir.y, ir.bottom() - 1, color);
            }
            return;
        }
        const x0: i32 = r.x;
        const y0: i32 = r.y;
        const x1: i32 = r.x + r.w;
        const y1: i32 = r.y + r.h;
        const t: i32 = @max(1, thickness);
        var y: i32 = y0;
        while (y < y1) : (y += 1) {
            var x: i32 = x0;
            while (x < x1) : (x += 1) {
                if (!insideRounded(x, y, x0, y0, x1, y1, rr)) continue;
                if (!filled and insideRounded(x, y, x0 + t, y0 + t, x1 - t, y1 - t, @max(0, rr - t))) continue;
                s.put(x, y, color);
            }
        }
    }

    fn insideRounded(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32, rr: i32) bool {
        if (x < x0 or x >= x1 or y < y0 or y >= y1) return false;
        if (rr <= 0) return true;
        const cxl: i32 = x0 + rr;
        const cxr: i32 = x1 - rr;
        const cyt: i32 = y0 + rr;
        const cyb: i32 = y1 - rr;
        const cx: i32 = if (x < cxl) cxl else if (x >= cxr) cxr else x;
        const cy: i32 = if (y < cyt) cyt else if (y >= cyb) cyb else y;
        const dx: i32 = x - cx;
        const dy: i32 = y - cy;
        return dx * dx + dy * dy <= rr * rr;
    }

    pub fn line(s: *Surface, x1: i32, y1: i32, x2: i32, y2: i32, color: Color) void {
        // Bresenham.
        var x: i32 = x1;
        var y: i32 = y1;
        const dx: i32 = @intCast(@abs(x2 - x1));
        const dy: i32 = -@as(i32, @intCast(@abs(y2 - y1)));
        const sx: i32 = if (x1 < x2) 1 else -1;
        const sy: i32 = if (y1 < y2) 1 else -1;
        var err: i32 = dx + dy;
        while (true) {
            s.put(x, y, color);
            if (x == x2 and y == y2) break;
            const e2: i32 = 2 * err;
            if (e2 >= dy) {
                err += dy;
                x += sx;
            }
            if (e2 <= dx) {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn lines(s: *Surface, points: []const Point, color: Color) void {
        if (points.len < 2) return;
        for (points[0 .. points.len - 1], 1..) |p, i| {
            line(s, p.x, p.y, points[i].x, points[i].y, color);
        }
    }

    /// A line `width` pixels thick (pygame's `width` argument).
    pub fn thick_line(s: *Surface, x1: i32, y1: i32, x2: i32, y2: i32, color: Color, width: i32) void {
        if (width <= 1) {
            line(s, x1, y1, x2, y2, color);
            return;
        }
        const hw: i32 = @divTrunc(width, 2);
        var x: i32 = x1;
        var y: i32 = y1;
        const dx: i32 = @intCast(@abs(x2 - x1));
        const dy: i32 = -@as(i32, @intCast(@abs(y2 - y1)));
        const sx: i32 = if (x1 < x2) 1 else -1;
        const sy: i32 = if (y1 < y2) 1 else -1;
        var err: i32 = dx + dy;
        while (true) {
            s.fill_rect(Rect.init(x - hw, y - hw, width, width), color);
            if (x == x2 and y == y2) break;
            const e2: i32 = 2 * err;
            if (e2 >= dy) {
                err += dy;
                x += sx;
            }
            if (e2 <= dx) {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn circle(s: *Surface, cx: i32, cy: i32, radius: i32, color: Color, filled: bool) void {
        ellipse(s, Rect.init(cx - radius, cy - radius, radius * 2, radius * 2), color, filled);
    }

    /// Circle/ellipse outline `thickness` pixels wide (solid ring, like
    /// pygame's `width`), or filled.
    pub fn ellipse_w(s: *Surface, r: Rect, color: Color, filled: bool, thickness: i32) void {
        if (filled or r.w <= 0 or r.h <= 0) {
            if (filled) ellipse(s, r, color, true);
            return;
        }
        const t: f32 = @floatFromInt(@max(1, thickness));
        const rx: f32 = @as(f32, @floatFromInt(r.w)) / 2.0;
        const ry: f32 = @as(f32, @floatFromInt(r.h)) / 2.0;
        if (rx < 0.5 or ry < 0.5) return;
        const cx: f32 = @as(f32, @floatFromInt(r.x)) + rx;
        const irx: f32 = @max(0.0, rx - t);
        const iry: f32 = @max(0.0, ry - t);
        var y: i32 = 0;
        while (y < r.h) : (y += 1) {
            const dy: f32 = @as(f32, @floatFromInt(r.y + y)) + 0.5 -
                (@as(f32, @floatFromInt(r.y)) + ry);
            const ty: f32 = 1.0 - (dy * dy) / (ry * ry);
            if (ty < 0) continue;
            const half_o: f32 = rx * @sqrt(ty);
            var half_i: f32 = 0;
            if (irx > 0 and iry > 0) {
                const ti: f32 = 1.0 - (dy * dy) / (iry * iry);
                if (ti > 0) half_i = irx * @sqrt(ti);
            }
            const row: i32 = r.y + y;
            if (half_i <= 0) {
                hline(s, @intFromFloat(cx - half_o), @intFromFloat(cx + half_o), row, color);
            } else {
                const ix0: i32 = @intFromFloat(cx - half_i);
                const ix1: i32 = @intFromFloat(cx + half_i);
                hline(s, @intFromFloat(cx - half_o), ix0 - 1, row, color);
                hline(s, ix1 + 1, @intFromFloat(cx + half_o), row, color);
            }
        }
    }

    /// Elliptical arc from `start` to `stop` radians (0 = 3 o'clock, CCW),
    /// `thickness` pixels wide. pygame's `draw.arc`.
    pub fn arc(s: *Surface, r: Rect, color: Color, start: f32, stop: f32, thickness: i32) void {
        const rx: f32 = @as(f32, @floatFromInt(r.w)) / 2.0;
        const ry: f32 = @as(f32, @floatFromInt(r.h)) / 2.0;
        if (rx < 0.5 or ry < 0.5) return;
        const cx: f32 = @as(f32, @floatFromInt(r.x)) + rx;
        const cy: f32 = @as(f32, @floatFromInt(r.y)) + ry;
        const span: f32 = stop - start;
        const radius: f32 = @max(rx, ry);
        const steps_i: i32 = @intFromFloat(@max(8.0, @abs(span) * radius * 0.6));
        const n: usize = @intCast(@max(8, steps_i));
        const t: i32 = @max(1, thickness);
        var i: usize = 0;
        var prev_x: i32 = 0;
        var prev_y: i32 = 0;
        var have: bool = false;
        while (i <= n) : (i += 1) {
            const a: f32 = start + span * (@as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(n)));
            const px: i32 = @intFromFloat(cx + rx * @cos(a));
            const py: i32 = @intFromFloat(cy - ry * @sin(a));
            if (have) thick_line(s, prev_x, prev_y, px, py, color, t);
            prev_x = px;
            prev_y = py;
            have = true;
        }
    }

    pub fn ellipse(s: *Surface, r: Rect, color: Color, filled: bool) void {
        const rx: f32 = @as(f32, @floatFromInt(r.w)) / 2.0;
        const ry: f32 = @as(f32, @floatFromInt(r.h)) / 2.0;
        if (rx < 0.5 or ry < 0.5) return;
        const cx: f32 = @as(f32, @floatFromInt(r.x)) + rx;
        const cy: f32 = @as(f32, @floatFromInt(r.y)) + ry;
        var y: i32 = 0;
        const y0: i32 = r.y;
        const y1: i32 = r.bottom();
        while (y0 + y < y1) : (y += 1) {
            const fy: f32 = @as(f32, @floatFromInt(y0 + y)) + 0.5 - cy;
            const t: f32 = 1.0 - (fy * fy) / (ry * ry);
            if (t < 0) continue;
            const half: i32 = @intFromFloat(rx * @sqrt(t));
            const px: i32 = @intFromFloat(cx - 0.5);
            if (filled) {
                hline(s, px - half, px + half, y0 + y, color);
            } else {
                // left/right extremes per scanline trace the whole outline
                s.put(px - half, y0 + y, color);
                s.put(px + half, y0 + y, color);
            }
        }
    }

    /// Filled convex/concave polygon (even-odd scanline fill).
    /// Filled polygon with sub-pixel (float) vertices. This is what pygame
    /// uses for the road: thin far quads vanish if the y coordinates are
    /// truncated to integers.
    pub fn polygon_f(s: *Surface, points: []const PointF, color: Color) void {
        if (points.len < 3) return;
        var min_y: f32 = points[0].y;
        var max_y: f32 = points[0].y;
        for (points) |p| {
            min_y = @min(min_y, p.y);
            max_y = @max(max_y, p.y);
        }
        var xs: [64]f32 = undefined;
        var drew = false;
        var y: i32 = @intFromFloat(@floor(min_y));
        const y_end: i32 = @intFromFloat(@ceil(max_y));
        while (y <= y_end) : (y += 1) {
            const scan: f32 = @as(f32, @floatFromInt(y)) + 0.5;
            var n: usize = 0;
            var i: usize = 0;
            while (i < points.len) : (i += 1) {
                const a: PointF = points[i];
                const b: PointF = points[(i + 1) % points.len];
                if ((a.y <= scan and b.y > scan) or (b.y <= scan and a.y > scan)) {
                    if (n < xs.len) {
                        xs[n] = a.x + (scan - a.y) / (b.y - a.y) * (b.x - a.x);
                        n += 1;
                    }
                }
            }
            var a2: usize = 1;
            while (a2 < n) : (a2 += 1) {
                const key = xs[a2];
                var b2: usize = a2;
                while (b2 > 0 and xs[b2 - 1] > key) : (b2 -= 1) xs[b2] = xs[b2 - 1];
                xs[b2] = key;
            }
            var k: usize = 0;
            while (k + 1 < n) : (k += 2) {
                hline(s, @intFromFloat(xs[k]), @intFromFloat(xs[k + 1]), y, color);
                drew = true;
            }
        }
        // Only stroke the edges when the scanline fill drew nothing — that is
        // the very thin (far) polygon case; thick ones are already filled.
        if (drew) return;
        var e: usize = 0;
        while (e < points.len) : (e += 1) {
            const a: PointF = points[e];
            const b: PointF = points[(e + 1) % points.len];
            line(s, @intFromFloat(a.x), @intFromFloat(a.y), @intFromFloat(b.x), @intFromFloat(b.y), color);
        }
    }

    pub fn polygon(s: *Surface, points: []const Point, color: Color) void {
        if (points.len < 3) return;
        var min_y: i32 = points[0].y;
        var max_y: i32 = points[0].y;
        for (points) |p| {
            min_y = @min(min_y, p.y);
            max_y = @max(max_y, p.y);
        }
        var xs: [64]f32 = undefined;
        var y: i32 = min_y;
        while (y <= max_y) : (y += 1) {
            const scan: f32 = @as(f32, @floatFromInt(y)) + 0.5;
            var n: usize = 0;
            var i: usize = 0;
            while (i < points.len) : (i += 1) {
                const a: Point = points[i];
                const b: Point = points[(i + 1) % points.len];
                const ay: f32 = @floatFromInt(a.y);
                const by: f32 = @floatFromInt(b.y);
                if ((ay <= scan and by > scan) or (by <= scan and ay > scan)) {
                    const ax: f32 = @floatFromInt(a.x);
                    const bx: f32 = @floatFromInt(b.x);
                    if (n < xs.len) {
                        xs[n] = ax + (scan - ay) / (by - ay) * (bx - ax);
                        n += 1;
                    }
                }
            }
            // Insertion sort: n is tiny (vertex count), and this avoids the
            // generic sort's call overhead per scanline.
            var a: usize = 1;
            while (a < n) : (a += 1) {
                const key = xs[a];
                var b: usize = a;
                while (b > 0 and xs[b - 1] > key) : (b -= 1) xs[b] = xs[b - 1];
                xs[b] = key;
            }
            var k: usize = 0;
            while (k + 1 < n) : (k += 2) {
                hline(s, @intFromFloat(xs[k]), @intFromFloat(xs[k + 1]), y, color);
            }
        }
    }

    fn hline(s: *Surface, x0: i32, x1: i32, y: i32, color: Color) void {
        if (y < 0 or y >= @as(i32, @intCast(s.h))) return;
        const start: i32 = @max(0, x0);
        const end: i32 = @min(@as(i32, @intCast(s.w)) - 1, x1);
        if (end < start) return;
        const base: usize = @as(usize, @intCast(y)) * @as(usize, s.w);
        const a: usize = @intCast(start);
        const b: usize = @intCast(end + 1);
        @memset(s.pixels[base + a .. base + b], pack(color));
    }

    fn vline(s: *Surface, x: i32, y0: i32, y1: i32, color: Color) void {
        var y: i32 = @max(0, y0);
        const end: i32 = @min(@as(i32, @intCast(s.h)) - 1, y1);
        while (y <= end) : (y += 1) s.put(x, y, color);
    }
};

// ── transform ────────────────────────────────────────────────────────────────

/// pygame.transform: nearest-neighbour scale and flip.
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

// ── display ──────────────────────────────────────────────────────────────────

/// The canvas currently presented to the window.
var canvas: ?*Surface = null;
var canvas_tex: ?neko.TextureHandle = null;
var logical_ready: bool = false;

/// No-op, for pygame-style `pygame.init()`.
pub fn init() void {}
/// No-op, for pygame-style `pygame.quit()`.
pub fn quit() void {}

fn create_mode(w: u32, h: u32) *Surface {
    const alloc: Allocator = std.heap.c_allocator;
    const s: *Surface = alloc.create(Surface) catch @panic("neko_pygame: out of memory");
    s.* = Surface.init(alloc, w, h) catch @panic("neko_pygame: out of memory");
    canvas = s;
    return s;
}

fn present_flip() void {
    const s: *Surface = canvas orelse return;
    if (!logical_ready) {
        neko.screen.set_logical_size(s.w, s.h);
        canvas_tex = neko.texture.create(s.w, s.h);
        logical_ready = true;
    }
    const tex: neko.TextureHandle = canvas_tex orelse return;
    neko.texture.update(tex, std.mem.sliceAsBytes(s.pixels), s.w * 4);
    neko.texture.draw(tex, neko.Rect.init(0, 0, @intCast(s.w), @intCast(s.h)), null, null);
}

/// Creates the canvas and remembers it; returns it so you can draw right away.
pub fn set_mode(w: u32, h: u32) *Surface {
    return create_mode(w, h);
}

/// The canvas returned by `set_mode`.
pub fn surface() *Surface {
    return canvas orelse @panic("neko_pygame: call set_mode() first");
}

/// Uploads the canvas to the GPU and scales it to the window (pygame `flip`).
pub fn flip() void {
    present_flip();
}

/// `pygame.display` — the same three calls under the pygame name.
pub const display: type = struct {
    pub fn set_mode(w: u32, h: u32) *Surface {
        return create_mode(w, h);
    }
    pub fn flip() void {
        present_flip();
    }
    pub fn get_surface() *Surface {
        return canvas orelse @panic("neko_pygame: call set_mode() first");
    }
};
