// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: hello — the least-boilerplate Neko program.
//!
//! No game struct, no manual loop: hand `neko.run` a single `update(dt)`
//! callback and use `neko.` directly. Arrows move the square; Escape quits.
//!
//! Run it with:
//!
//!     zig build run-hello

const std: type = @import("std");
const neko: type = @import("neko");

var x: f32 = 300;
var y: f32 = 160;

pub fn main(init: std.process.Init) !void {
    try neko.run(init, .{
        .title = "Neko — Hello",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    }, update);
}

fn update(dt: f32) void {
    if (neko.input.keyDown(.escape)) neko.quit();

    if (neko.input.key(.left)) {
        x -= 160 * dt;
    }
    if (neko.input.key(.right)) {
        x += 160 * dt;
    }
    if (neko.input.key(.up)) {
        y -= 160 * dt;
    }
    if (neko.input.key(.down)) {
        y += 160 * dt;
    }

    neko.draw.rect(
        neko.Rect.init(@intFromFloat(x), @intFromFloat(y), 40, 40),
        neko.Color.hex(0x66ccff),
        true,
    );
    neko.text.draw("Arrow keys move · Escape quits", 320, 40, .{
        .size = 20,
        .center = true,
        .color = neko.Color.hex(0x8888aa),
    });
    neko.debug.draw_fps(8, 8, neko.Color.yellow);
}
