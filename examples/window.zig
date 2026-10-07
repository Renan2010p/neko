// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: window — an explicit window object and a `switch` over events.
//!
//! If you prefer to own the loop, create a `neko.window.Window`, poll events
//! with `poll_event` and `switch` on them. `begin_frame` drains the queue,
//! updates the input state and computes `dt`. Arrows move the square.
//!
//! Run it with:
//!
//!     zig build run-window

const std: type = @import("std");
const neko: type = @import("neko");

var x: f32 = 300;

pub fn main(init: std.process.Init) !void {
    var window: neko.Window = try neko.window.create(init, .{
        .title = "Neko — Window",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
    defer window.close();

    while (window.is_open()) {
        // Drain events once; this also updates neko.input's state and dt.
        window.begin_frame();

        while (window.poll_event()) |ev| {
            switch (ev) {
                .quit => window.close(),
                .key_down => |key| switch (key.key) {
                    .escape => window.close(),
                    else => {},
                },
                .mouse_button_down => |button| {
                    if (button.button == .left) x = @floatFromInt(button.x);
                },
                else => {},
            }
        }

        // The window may have closed while handling events.
        if (!window.is_open()) break;

        if (neko.input.key(.left)) x -= 200 * window.dt();
        if (neko.input.key(.right)) x += 200 * window.dt();

        neko.draw.clear(neko.Color.hex(0x0c0c12));
        neko.draw.rect(
            neko.Rect.init(@intFromFloat(x), 160, 40, 40),
            neko.Color.hex(0x66ccff),
            true,
        );
        neko.text.draw("window + switch(event)", 320, 40, .{
            .size = 20,
            .center = true,
            .color = neko.Color.hex(0x8888aa),
        });

        window.present();
        window.end_frame();
    }
}
