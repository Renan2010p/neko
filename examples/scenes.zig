// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: scenes — a window with switchable scenes (Godot-style).
//!
//! Build a scene with `neko.scene.create` and add nodes to it. `window.switch_to`
//! makes a scene current and **frees the previous one**, exactly like Godot's
//! `change_scene`: to go back, build that scene again. Enter goes menu → play;
//! Escape goes play → menu, and quits from the menu.
//!
//! Run it with:
//!
//!     zig build run-scenes

const std: type = @import("std");
const neko: type = @import("neko");

/// A scene is a root node (alias `neko.Scene`).
fn makeMenu() !*neko.Scene {
    const root: *neko.Scene = try neko.scene.create(neko.allocator(), .{ .name = "menu" });
    _ = try neko.scene.addLabel(root, .{
        .text = "NEKO",
        .pos = .{ .x = 320, .y = 90 },
        .size = 54,
        .center = true,
        .color = neko.Color.hex(0x66ccff),
    });
    _ = try neko.scene.addLabel(root, .{
        .text = "Enter: play",
        .pos = .{ .x = 320, .y = 190 },
        .size = 24,
        .center = true,
    });
    _ = try neko.scene.addLabel(root, .{
        .text = "Escape: quit",
        .pos = .{ .x = 320, .y = 226 },
        .size = 16,
        .center = true,
        .color = neko.Color.hex(0x8888aa),
    });
    return root;
}

fn makePlay() !*neko.Scene {
    const root: *neko.Scene = try neko.scene.create(neko.allocator(), .{ .name = "play" });
    _ = try neko.scene.addSprite(root, .{
        .image = "player",
        .pos = .{ .x = 288, .y = 150 },
        .size = .{ .x = 64, .y = 64 },
    });
    _ = try neko.scene.addLabel(root, .{
        .text = "Play — Escape: menu",
        .pos = .{ .x = 320, .y = 40 },
        .size = 22,
        .center = true,
    });
    return root;
}

fn isCurrent(name: []const u8) bool {
    return std.mem.eql(u8, neko.scene.name(), name);
}

pub fn main(init: std.process.Init) !void {
    var window: neko.Window = try neko.window.create(init, .{
        .title = "Neko — Scenes",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
    defer window.close();

    window.set_clear_color(neko.Color.hex(0x101018));
    window.switch_to(try makeMenu());

    while (window.is_open()) {
        window.begin_frame();

        while (window.poll_event()) |ev| {
            switch (ev) {
                .quit => window.close(),
                .key_down => |key| switch (key.key) {
                    .escape => {
                        if (isCurrent("menu")) {
                            window.close();
                        } else {
                            window.switch_to(try makeMenu());
                        }
                    },
                    .enter => {
                        if (isCurrent("menu")) window.switch_to(try makePlay());
                    },
                    else => {},
                },
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
