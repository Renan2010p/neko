// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `draw` — pygame-shaped primitives that render *into* a `Surface`.

const neko: type = @import("neko");
const surface_mod: type = @import("surface.zig");
const rect_mod: type = @import("rect.zig");
const pixel: type = @import("pixel.zig");

const Surface: type = surface_mod.Surface;
const Rect: type = rect_mod.Rect;
const Point: type = rect_mod.Point;
const Color: type = neko.Color;
const PointF: type = pixel.PointF;

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

    /// Filled polygon with integer vertices (even-odd scanline fill).
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
        @memset(s.pixels[base + a .. base + b], pixel.pack(color));
    }

    fn vline(s: *Surface, x: i32, y0: i32, y1: i32, color: Color) void {
        var y: i32 = @max(0, y0);
        const end: i32 = @min(@as(i32, @intCast(s.h)) - 1, y1);
        while (y <= end) : (y += 1) s.put(x, y, color);
    }
};
