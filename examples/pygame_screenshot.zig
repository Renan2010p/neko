// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example/tool: pygame screenshot — renders the compat demo scene to a PPM
//! file without opening a window, so it can run headless/CI.
//!
//!     zig build run-pygame_screenshot   # writes neko_shot.ppm

const std: type = @import("std");
const pg: type = @import("neko_pygame");

const W: u32 = 512;
const H: u32 = 288;

fn scene(s: *pg.Surface) void {
    s.fill(pg.Color.hex(0x0a0c16));
    pg.draw.rect(s, pg.Rect.init(0, 180, 512, 108), pg.Color.hex(0x181226), true);

    pg.draw.ellipse(s, pg.Rect.init(40, 88, 120, 72), pg.Color.rgba(90, 200, 255, 150), true);
    pg.draw.polygon(s, &.{
        pg.Point.init(300, 56), pg.Point.init(432, 118),
        pg.Point.init(360, 196), pg.Point.init(258, 150),
    }, pg.Color.hex(0xf0c040));
    pg.draw.circle(s, 150, 224, 26, pg.Color.hex(0x4fd97a), true);
    pg.draw.circle(s, 150, 224, 26, pg.Color.white, false);
    pg.draw.line(s, 20, 24, 492, 24, pg.Color.hex(0x5a6a8a));
    pg.draw.lines(s, &.{
        pg.Point.init(20, 262), pg.Point.init(120, 232), pg.Point.init(200, 266),
    }, pg.Color.hex(0xff6a4a));

    // a procedurally generated sprite, twice (one flipped)
    var hero: pg.Surface = pg.Surface.init(std.heap.c_allocator, 16, 22) catch @panic("oom");
    defer hero.deinit();
    pg.draw.rect(&hero, pg.Rect.init(5, 4, 6, 12), pg.Color.hex(0xc43a5c), true);
    pg.draw.circle(&hero, 8, 5, 3, pg.Color.hex(0xf5c462), true);
    pg.draw.circle(&hero, 11, 5, 1, pg.Color.hex(0x60f0ff), true);
    pg.draw.rect(&hero, pg.Rect.init(6, 16, 4, 5), pg.Color.hex(0x424460), true);

    var hero_flipped: ?pg.Surface = null;
    defer if (hero_flipped) |*f| f.deinit();
    s.blit(&hero, pg.Point.init(64, 196));
    const flipped: *const pg.Surface = pg.transform.maybeFlipX(&hero, true, &hero_flipped);
    s.blit(flipped, pg.Point.init(104, 196));

    // a scaled-up copy, to show transform.scale
    var big: pg.Surface = pg.transform.scale(&hero, 32, 44) catch return;
    defer big.deinit();
    s.blit(&big, pg.Point.init(160, 190));
}

pub fn main(init: std.process.Init) !void {
    var s: pg.Surface = try pg.Surface.init(std.heap.c_allocator, W, H);
    defer s.deinit();
    scene(&s);

    var header: [64]u8 = undefined;
    const head: []u8 = try std.fmt.bufPrint(&header, "P6\n{d} {d}\n255\n", .{ W, H });
    const out: []u8 = try std.heap.c_allocator.alloc(u8, head.len + W * H * 3);
    defer std.heap.c_allocator.free(out);
    @memcpy(out[0..head.len], head);
    var i: usize = head.len;
    var y: u32 = 0;
    while (y < H) : (y += 1) {
        var x: u32 = 0;
        while (x < W) : (x += 1) {
            const p: u32 = s.pixels[y * W + x];
            out[i] = @truncate(p >> 16);
            out[i + 1] = @truncate(p >> 8);
            out[i + 2] = @truncate(p);
            i += 3;
        }
    }

    const dir: std.Io.Dir = try std.Io.Dir.cwd().createDirPathOpen(init.io, ".", .{});
    try dir.writeFile(init.io, .{ .sub_path = "neko_shot.ppm", .data = out });
    std.debug.print("wrote neko_shot.ppm ({d}x{d})\n", .{ W, H });
}
