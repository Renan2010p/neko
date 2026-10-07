// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: shapes — the `neko.draw` primitives.
//!
//! Filled/outlined rectangles, lines, circles, a quad, a star and text.
//!
//! Run it with:
//!
//!     zig build run-shapes

const std: type = @import("std");
const neko: type = @import("neko");

const Game: type = struct {
    elapsed: f32 = 0,

    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| if (key.key == .escape) neko.lifecycle.request_stop(),
            else => {},
        }
    }

    pub fn update(self: *Game, dt: f32) void {
        self.elapsed += dt;
    }

    pub fn draw(self: *Game) void {
        // Rectangles.
        neko.draw.rect(neko.Rect.init(40, 60, 140, 80), neko.Color.hex(0xd94f4f), true);
        neko.draw.rect(neko.Rect.init(40, 60, 140, 80), neko.Color.white, false);

        // A line.
        neko.draw.line(220, 60, 360, 140, neko.Color.cyan);

        // Circles: outline + filled, with a pulsing radius.
        neko.draw.circle(460, 100, 40, neko.Color.hex(0x4fd97a), false);
        const pulse: f32 = 18 + neko.math.smoothstep(0, 1, @mod(self.elapsed, 2.0) / 2.0) * 18;
        neko.draw.circle(560, 100, @intFromFloat(pulse), neko.Color.hex(0x4fd97a), true);

        // A diamond built from a quad.
        neko.draw.quad(
            .{ .x = 80, .y = 170 },
            .{ .x = 150, .y = 240 },
            .{ .x = 80, .y = 310 },
            .{ .x = 10, .y = 240 },
            neko.Color.hex(0xf0c040),
        );

        // A star.
        neko.draw.star(260, 240, 60, neko.Color.hex(0xf0c040));

        // Labels.
        neko.text.draw("rect", 110, 200, .{ .size = 18, .center = true });
        neko.text.draw("quad", 80, 320, .{ .size = 18, .center = true });
        neko.text.draw("star", 260, 320, .{ .size = 18, .center = true });
        neko.text.draw("neko.draw", 560, 300, .{ .size = 24, .center = true, .color = neko.Color.white });
        neko.text.draw("Escape to quit", 560, 330, .{ .size = 16, .center = true, .color = neko.Color.hex(0x8888aa) });
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Shapes",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
