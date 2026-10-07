// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: splash — the NEKO boot splash.
//!
//! Shows the asset-free "NEKO / loading" screen with a progress bar. It uses
//! an embedded bitmap font, so it renders even with no `assets/font`. Any key
//! or mouse button skips it.
//!
//! `neko.run` and `neko.app.run` show this automatically (disable with
//! `.splash = null`); this example drives the explicit window API.
//!
//! Run it with:
//!
//!     zig build run-splash

const std: type = @import("std");
const neko: type = @import("neko");

pub fn main(init: std.process.Init) !void {
    var window: neko.Window = try neko.window.create(init, .{
        .title = "Neko — Splash",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
    defer window.close();

    // Boot splash before the game starts (skippable, asset-free).
    window.splash(.{ .duration_ms = 2000 });

    window.set_clear_color(neko.Color.hex(0x0c0c12));
    const root: *neko.Scene = try neko.scene.create(neko.allocator(), .{ .name = "world" });
    _ = try neko.scene.addLabel(root, .{
        .text = "Splash done — Escape quits",
        .pos = .{ .x = 320, .y = 40 },
        .size = 20,
        .center = true,
        .color = neko.Color.hex(0x8888aa),
    });
    window.switch_to(root);

    while (window.is_open()) {
        window.begin_frame();

        while (window.poll_event()) |ev| {
            switch (ev) {
                .quit => window.close(),
                .key_down => |key| if (key.key == .escape) window.close(),
                else => {},
            }
        }
        if (!window.is_open()) break;

        window.process(window.dt());
        window.draw();
        window.present();
        window.end_frame();
    }
}
