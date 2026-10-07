// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: app — the high-level `neko.app.run` runner.
//!
//! `neko.app.run` owns the loop (events → update → draw → present) and calls
//! only the callbacks your struct actually declares. Here a square bounces
//! around the window; Escape quits.
//!
//! Run it with:
//!
//!     zig build run-app

const std: type = @import("std");
const neko: type = @import("neko");

const Game: type = struct {
    x: f32 = 40,
    y: f32 = 60,
    vx: f32 = 140,
    vy: f32 = 95,
    size: f32 = 42,

    /// Called once, after the window is ready.
    pub fn start(self: *Game) void {
        neko.random.seed(7);
        self.vx = @floatFromInt(neko.random.int_range(90, 190));
        self.vy = @floatFromInt(neko.random.int_range(70, 150));
    }

    /// Called for every input/window event.
    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| if (key.key == .escape) neko.lifecycle.request_stop(),
            else => {},
        }
    }

    /// Called once per frame with the seconds since the last frame.
    pub fn update(self: *Game, dt: f32) void {
        const screen: neko.Point = neko.screen.logical_size();
        const w: f32 = @floatFromInt(screen.x);
        const h: f32 = @floatFromInt(screen.y);

        self.x += self.vx * dt;
        self.y += self.vy * dt;

        if (self.x < 0) {
            self.x = 0;
            self.vx = @abs(self.vx);
        } else if (self.x + self.size > w) {
            self.x = w - self.size;
            self.vx = -@abs(self.vx);
        }

        if (self.y < 0) {
            self.y = 0;
            self.vy = @abs(self.vy);
        } else if (self.y + self.size > h) {
            self.y = h - self.size;
            self.vy = -@abs(self.vy);
        }
    }

    /// Called after the runner clears the screen.
    pub fn draw(self: *Game) void {
        const rect: neko.Rect = .{
            .x = @intFromFloat(self.x),
            .y = @intFromFloat(self.y),
            .w = @intFromFloat(self.size),
            .h = @intFromFloat(self.size),
        };
        neko.draw.rect(rect, neko.Color.hex(0x66ccff), true);
        neko.debug.draw_fps(8, 8, neko.Color.yellow);
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — App runner",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
