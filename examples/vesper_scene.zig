// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! A slice of Vesper drawn with Neko — the Nara surface landing site.
//!
//! This is a hand-port of Vesper's procedural art (`vesper/game/art/*`) and
//! the `NARA_SURFACE` room grid, rendered through the `neko_pygame` layer.  It
//! is the first concrete step of the Python -> Zig rewrite (stage 2).

const std: type = @import("std");
const pg: type = @import("neko_pygame");

const TILE: i32 = 24;
const W: u32 = 512;
const H: u32 = 288;
const WI: i32 = @intCast(W);
const HI: i32 = @intCast(H);

// -- palette (from vesper/game/art/_base.py + config.py) ---------------------
const SUIT: pg.Color = pg.Color.rgb(196, 58, 92);
const SUIT_DARK: pg.Color = pg.Color.rgb(126, 30, 62);
const ARMOR: pg.Color = pg.Color.rgb(226, 230, 244);
const VISOR: pg.Color = pg.Color.rgb(96, 240, 255);
const HAIR: pg.Color = pg.Color.rgb(245, 196, 92);
const SKIN: pg.Color = pg.Color.rgb(236, 190, 150);
const BOOT: pg.Color = pg.Color.rgb(66, 68, 96);
const GUN: pg.Color = pg.Color.rgb(208, 214, 232);
const BUG: pg.Color = pg.Color.rgb(96, 168, 96);
const BUG_DARK: pg.Color = pg.Color.rgb(52, 104, 66);
const BUG_EYE: pg.Color = pg.Color.rgb(255, 90, 80);

const ROCK: pg.Color = pg.Color.rgb(88, 76, 64);
const ROCK_TOP: pg.Color = pg.Color.rgb(112, 98, 80);
const ROCK_DARK: pg.Color = pg.Color.rgb(58, 50, 44);
const LEDGE: pg.Color = pg.Color.rgb(124, 112, 92);

/// The landing room, straight from `vesper/game/rooms.py` (`NARA_SURFACE`).
const NARA_SURFACE: [12][]const u8 = [_][]const u8{
    "########################",
    "#                      #",
    "#                      #",
    "#                      #",
    "#        NNNNN         #",
    "#                      #",
    "#                      #",
    "#                      #",
    "#                      #",
    "#        =====         #",
    "#                      #",
    "########################",
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn vgrad(s: *pg.Surface, y0: i32, y1: i32, top: pg.Color, bottom: pg.Color) void {
    var y: i32 = y0;
    while (y < y1) : (y += 1) {
        const t: f32 = @as(f32, @floatFromInt(y - y0)) / @as(f32, @floatFromInt(@max(1, y1 - y0 - 1)));
        const c: pg.Color = pg.Color.lerp(top, bottom, t);
        pg.draw.line(s, 0, y, @intCast(W), y, c);
    }
}

fn starfield(s: *pg.Surface, t: f32) void {
    var rng: std.Random.DefaultPrng = std.Random.DefaultPrng.init(7);
    const r: std.Random = rng.random();
    var i: usize = 0;
    while (i < 90) : (i += 1) {
        const x: i32 = @intFromFloat(r.float(f32) * @as(f32, @floatFromInt(W)));
        const y: i32 = @intFromFloat(r.float(f32) * @as(f32, @floatFromInt(H)) * 0.7);
        const tw: f32 = 0.5 + 0.5 * @sin(t * 3.0 + @as(f32, @floatFromInt(i)));
        const c: u8 = @intFromFloat(90 + 120 * tw);
        s.set_pixel(x, y, pg.Color.rgb(c, c, @min(255, c +% 20)));
    }
}

fn rain(s: *pg.Surface, t: f32) void {
    var i: i32 = 0;
    while (i < 60) : (i += 1) {
        const x: i32 = @mod(i * 61 + @as(i32, @intFromFloat(t * 500.0)), WI + 40) - 20;
        const y: i32 = @mod(i * 97 + @as(i32, @intFromFloat(t * 900.0)), HI + 40) - 20;
        pg.draw.line(s, x, y, x - 4, y + 16, pg.Color.rgb(120, 150, 190));
    }
}

fn black_hole(s: *pg.Surface, cx: i32, cy: i32, radius: i32, t: f32) void {
    const half: i32 = @divTrunc(radius, 2);
    // accretion ring
    pg.draw.ellipse(s, pg.Rect.init(cx - radius * 2, cy - half, radius * 4, radius), pg.Color.rgba(255, 170, 90, 120), false);
    pg.draw.ellipse(s, pg.Rect.init(cx - radius * 2, cy - half, radius * 4, radius), pg.Color.rgba(255, 220, 150, 90), false);
    // event horizon
    pg.draw.circle(s, cx, cy, radius, pg.Color.rgb(4, 4, 8), true);
    // a few orbiting sparks
    var i: usize = 0;
    while (i < 10) : (i += 1) {
        const a: f32 = t * 0.6 + @as(f32, @floatFromInt(i)) * 0.628;
        const rr: f32 = @as(f32, @floatFromInt(radius)) * 1.6;
        const x: i32 = cx + @as(i32, @intFromFloat(@cos(a) * rr));
        const y: i32 = cy + @as(i32, @intFromFloat(@sin(a) * rr * 0.35));
        s.set_pixel(x, y, pg.Color.rgb(255, 210, 150));
    }
}

// ── tiles ────────────────────────────────────────────────────────────────────

fn draw_tile(s: *pg.Surface, x: i32, y: i32, kind: u8) void {
    switch (kind) {
        '#' => {
            s.fill_rect(pg.Rect.init(x, y, TILE, TILE), ROCK);
            s.fill_rect(pg.Rect.init(x, y, TILE, 3), ROCK_TOP);
            s.fill_rect(pg.Rect.init(x, y + TILE - 3, TILE, 3), ROCK_DARK);
        },
        '=' => {
            s.fill_rect(pg.Rect.init(x, y, TILE, 6), LEDGE);
            s.fill_rect(pg.Rect.init(x, y, TILE, 2), ROCK_TOP);
        },
        else => {},
    }
}

// ── sprites ──────────────────────────────────────────────────────────────────

fn hunter(s: *pg.Surface, x: i32, y: i32, leg: i32) void {
    // legs
    s.fill_rect(pg.Rect.init(x + 2, y + 15, 4, 6 + leg), BOOT);
    s.fill_rect(pg.Rect.init(x + 8, y + 15, 4, 6 - leg), BOOT);
    // body
    s.fill_rect(pg.Rect.init(x + 2, y + 8, 10, 9), SUIT);
    s.fill_rect(pg.Rect.init(x + 2, y + 15, 10, 2), SUIT_DARK);
    // arm + gun
    s.fill_rect(pg.Rect.init(x + 10, y + 11, 7, 3), SUIT);
    s.fill_rect(pg.Rect.init(x + 15, y + 11, 4, 2), GUN);
    // head
    pg.draw.circle(s, x + 7, y + 5, 4, SKIN, true);
    s.fill_rect(pg.Rect.init(x + 3, y + 1, 9, 3), HAIR);
    s.fill_rect(pg.Rect.init(x + 6, y + 4, 5, 2), VISOR);
    // shoulder plate
    s.fill_rect(pg.Rect.init(x + 1, y + 8, 3, 4), ARMOR);
}

fn crawler(s: *pg.Surface, x: i32, y: i32, phase: i32) void {
    s.fill_rect(pg.Rect.init(x + 2, y + 5, 12, 6), BUG);
    s.fill_rect(pg.Rect.init(x + 2, y + 5, 12, 2), BUG_DARK);
    s.fill_rect(pg.Rect.init(x + 11, y + 6, 3, 3), BUG_EYE);
    var i: i32 = 0;
    while (i < 4) : (i += 1) {
        const off: i32 = if (@mod(i + phase, 2) == 0) 1 else 0;
        s.fill_rect(pg.Rect.init(x + 3 + i * 3, y + 11 - off, 1, 3), BUG_DARK);
    }
}

/// Vesper's gunship, parked (ported from `art/ships.py`).
fn gunship(s: *pg.Surface, x: i32, y: i32) void {
    const hull: pg.Color = pg.Color.rgb(190, 196, 214);
    const hull_dark: pg.Color = pg.Color.rgb(96, 104, 130);
    const accent: pg.Color = pg.Color.rgb(90, 240, 255);
    // landing struts
    s.fill_rect(pg.Rect.init(x + 34, y + 40, 5, 18), hull_dark);
    s.fill_rect(pg.Rect.init(x + 96, y + 40, 5, 18), hull_dark);
    s.fill_rect(pg.Rect.init(x + 24, y + 56, 25, 5), hull_dark);
    s.fill_rect(pg.Rect.init(x + 86, y + 56, 25, 5), hull_dark);
    // hull (a coarse polygon via horizontal spans)
    pg.draw.polygon(s, &.{
        pg.Point.init(x + 24, y + 18), pg.Point.init(x + 44, y + 10),
        pg.Point.init(x + 98, y + 8), pg.Point.init(x + 120, y + 20),
        pg.Point.init(x + 114, y + 31), pg.Point.init(x + 48, y + 33),
    }, hull_dark);
    pg.draw.polygon(s, &.{
        pg.Point.init(x + 26, y + 19), pg.Point.init(x + 46, y + 13),
        pg.Point.init(x + 96, y + 11), pg.Point.init(x + 114, y + 20),
        pg.Point.init(x + 108, y + 28), pg.Point.init(x + 50, y + 30),
    }, hull);
    // cockpit
    pg.draw.polygon(s, &.{
        pg.Point.init(x + 90, y + 12), pg.Point.init(x + 110, y + 18),
        pg.Point.init(x + 100, y + 25), pg.Point.init(x + 86, y + 20),
    }, accent);
    // boarding ramp light
    pg.draw.line(s, x + 50, y + 46, x + 86, y + 46, accent);
}

// ── scene ────────────────────────────────────────────────────────────────────

pub fn draw(s: *pg.Surface, t: f32) void {
    // sky: the "surface" zone colours
    vgrad(s, 0, @intCast(H - 40), pg.Color.rgb(8, 10, 24), pg.Color.rgb(36, 30, 48));
    starfield(s, t);
    black_hole(s, 300, 60, 26, t);
    rain(s, t);

    // the room, scrolled so the gunship sits near the middle
    const off_x: i32 = 8;
    const off_y: i32 = HI - 12 * TILE; // room is 12 tiles tall
    for (NARA_SURFACE, 0..) |row, ry| {
        for (row, 0..) |ch, cx| {
            draw_tile(s, off_x + @as(i32, @intCast(cx)) * TILE,
                off_y + @as(i32, @intCast(ry)) * TILE, ch);
        }
    }

    // the gunship (N run centre column 11 -> x = 264; bottom row 11 -> y = 264)
    gunship(s, off_x + 264 - 68, off_y + 264 - 64);

    // the heroine, standing on the floor with a little idle bob
    const bob: i32 = @intFromFloat(@sin(t * 2.0) * 1.5);
    hunter(s, off_x + 168, off_y + 216 + bob, @intFromFloat(@sin(t * 4.0) * 1.5));

    // a crawler wandering on the floor
    const cx: i32 = off_x + 360 + @as(i32, @intFromFloat(@sin(t * 0.8) * 30.0));
    crawler(s, cx, off_y + 258, @as(i32, @intFromFloat(t * 6.0)) & 1);
}
