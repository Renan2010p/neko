// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: script — attach a plain object to the scene as a node.
//!
//! This is the code-first equivalent of "attaching a script" in Godot: define
//! a struct with (optional, public) `ready`/`process`/`draw`/`input` methods
//! and add it with `neko.scene.addScript`. Find nodes later with
//! `neko.scene.get_node("Player")`.
//!
//! Run it with:
//!
//!     zig build run-script

const std: type = @import("std");
const neko: type = @import("neko");

/// The "script": gameplay lives here, not in a vtable.
const Player: type = struct {
    x: f32 = 300,
    y: f32 = 160,
    speed: f32 = 180,

    pub fn ready(self: *Player) void {
        // Called once, when the node first processes.
        std.debug.print("Player ready at ({d}, {d})\n", .{ self.x, self.y });
    }

    pub fn process(self: *Player, dt: f32) void {
        if (neko.input.key(.left)) self.x -= self.speed * dt;
        if (neko.input.key(.right)) self.x += self.speed * dt;
        if (neko.input.key(.up)) self.y -= self.speed * dt;
        if (neko.input.key(.down)) self.y += self.speed * dt;
    }

    pub fn draw(self: *Player, at: neko.Point) void {
        const ox: f32 = @floatFromInt(at.x);
        const oy: f32 = @floatFromInt(at.y);
        neko.draw.rect(
            neko.Rect.init(@intFromFloat(self.x + ox), @intFromFloat(self.y + oy), 40, 40),
            neko.Color.hex(0x66ccff),
            true,
        );
    }
};

/// The object must outlive the node; a global (or a field of your game) is fine.
var player: Player = Player{};

pub fn main(init: std.process.Init) !void {
    var window: neko.Window = try neko.window.create(init, .{
        .title = "Neko — Script",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
    defer window.close();

    window.set_clear_color(neko.Color.hex(0x0c0c12));

    const root: *neko.Scene = try neko.scene.create(neko.allocator(), .{ .name = "world" });
    _ = try neko.scene.addScript(root, &player, .{ .name = "Player" });
    _ = try neko.scene.addLabel(root, .{
        .text = "Script node — arrows move",
        .pos = .{ .x = 320, .y = 30 },
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

        // ready() on the first frame, then process(dt) down the tree.
        window.process(window.dt());
        window.draw();
        window.present();
        window.end_frame();
    }
}
