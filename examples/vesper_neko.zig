// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: Vesper on Neko — the Nara surface landing site, drawn with the
//! `neko_pygame` layer (a slice of the stage-2 rewrite).
//!
//!     zig build run-vesper_neko

const std: type = @import("std");
const neko: type = @import("neko");
const pg: type = @import("neko_pygame");
const scene: type = @import("vesper_scene.zig");

const Game: type = struct {
    elapsed: f32 = 0,

    pub fn update(self: *Game, dt: f32) void {
        self.elapsed += dt;
    }

    pub fn draw(self: *Game) void {
        scene.draw(pg.surface(), self.elapsed);
        pg.display.flip();
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| if (key.key == .escape) neko.lifecycle.request_stop(),
            else => {},
        }
    }
};

pub fn main(init: std.process.Init) !void {
    _ = pg.display.set_mode(512, 288);
    var game: Game = Game{};
    try neko.app.runOptions(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Vesper — Nara Surface (Neko)",
        .width = 1024,
        .height = 576,
    }, .{ .clear_color = null, .load_default_font = false });
}
