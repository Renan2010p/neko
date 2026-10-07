// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example/tool: Vesper screenshot — renders the Nara surface scene to a PPM
//! file without a window (headless/CI).
//!
//!     zig build run-vesper_neko_shot   # writes vesper_neko.ppm

const std: type = @import("std");
const pg: type = @import("neko_pygame");
const scene: type = @import("vesper_scene.zig");

const W: u32 = 512;
const H: u32 = 288;

pub fn main(init: std.process.Init) !void {
    var s: pg.Surface = try pg.Surface.init(std.heap.c_allocator, W, H);
    defer s.deinit();
    scene.draw(&s, 0.8);

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
    try dir.writeFile(init.io, .{ .sub_path = "vesper_neko.ppm", .data = out });
    std.debug.print("wrote vesper_neko.ppm ({d}x{d})\n", .{ W, H });
}
