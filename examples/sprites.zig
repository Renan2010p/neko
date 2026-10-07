// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: sprites — named sprites, cached fonts and a widget.
//!
//! `neko.sprite.load("player")` loads `examples/assets/player.png`. If the
//! file is missing the engine draws a labelled placeholder, so this example
//! runs with or without assets. Q toggles render quality.
//!
//! Run it with:
//!
//!     zig build run-sprites

const std: type = @import("std");
const neko: type = @import("neko");

const Game: type = struct {
    pub fn start(self: *Game) void {
        _ = self;
        // Loaded once and cached; missing files become placeholders.
        neko.sprite.load("player");
        neko.sprite.load("logo");
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| switch (key.key) {
                .escape => neko.lifecycle.request_stop(),
                .q => neko.sprite.set_quality(if (neko.sprite.is_high_quality()) "low" else "high"),
                else => {},
            },
            else => {},
        }
    }

    pub fn draw(self: *Game) void {
        _ = self;
        neko.sprite.draw("player", 60, 60, 96, 96);
        neko.sprite.draw("logo", 200, 60, 120, 96);

        // A sprite drawn only if it exists (no placeholder).
        neko.sprite.face("missing_sprite", 360, 60, 64, 64);

        // A simple labelled button widget.
        neko.sprite.button(
            60,
            220,
            200,
            48,
            "Start",
            true,
            neko.Color.hex(0x2f6f4f),
            neko.Color.hex(0x333344),
        );

        var buf: [64]u8 = undefined;
        const line: []const u8 = std.fmt.bufPrint(&buf, "quality: {s}", .{neko.sprite.quality()}) catch "";
        neko.text.draw(line, 320, 236, .{ .size = 22 });
        neko.text.draw("Q toggles quality · Escape quits", 320, 320, .{ .size = 16, .color = neko.Color.hex(0x8888aa) });
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Sprites",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
