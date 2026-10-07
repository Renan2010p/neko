// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: scene_tree — a Godot-style node tree.
//!
//! Builds a root `Node`, attaches a `Label`, a `Sprite` and a repeating
//! `Timer`, and drives the tree manually (events → process → draw → present).
//! Move the sprite with the arrow keys; the timer renames the title.
//!
//! Run it with:
//!
//!     zig build run-scene_tree

const std: type = @import("std");
const neko: type = @import("neko");

const titles: [3][]const u8 = [_][]const u8{ "Scene tree", "Timer fired!", "Nodes all the way down" };

pub fn main(init: std.process.Init) !void {
    const config: neko.Config = .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Scene tree",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    };

    if (!neko.screen.init(config)) return error.InitFailed;
    defer neko.screen.shutdown();

    // The manager owns the stack and frees the roots on deinit.
    neko.scene.init(init.gpa);
    defer neko.scene.deinit();
    neko.scene.set_clear_color(neko.Color.hex(0x101018));

    const root: *neko.Scene = try neko.scene.create(init.gpa, .{ .name = "root" });
    const title: *neko.scene.Label = try neko.scene.addLabel(root, .{
        .text = titles[0],
        .pos = .{ .x = 320, .y = 36 },
        .size = 30,
        .center = true,
    });
    const hero: *neko.scene.Sprite = try neko.scene.addSprite(root, .{
        .image = "player",
        .pos = .{ .x = 288, .y = 160 },
        .size = .{ .x = 64, .y = 64 },
    });
    const timer: *neko.scene.Timer = try neko.scene.addTimer(root, .{ .wait_time = 1.0, .one_shot = false });

    neko.scene.switch_to(root);

    var title_index: usize = 0;
    var last_ms: u64 = neko.time.ticks_ms();

    while (neko.lifecycle.keeps_running()) {
        while (neko.input.poll_event()) |ev| {
            switch (ev) {
                .quit => neko.lifecycle.request_stop(),
                .key_down => |key| switch (key.key) {
                    .escape => neko.lifecycle.request_stop(),
                    .left => hero.node.pos.x -= 8,
                    .right => hero.node.pos.x += 8,
                    .up => hero.node.pos.y -= 8,
                    .down => hero.node.pos.y += 8,
                    else => {},
                },
                else => {},
            }
            // Let the nodes observe the event too.
            neko.scene.input(ev);
        }

        const now_ms: u64 = neko.time.ticks_ms();
        const dt: f32 = @as(f32, @floatFromInt(now_ms - last_ms)) / 1000.0;
        last_ms = now_ms;

        // ready() on first call, then process() recursively.
        neko.scene.process(dt);

        if (timer.is_timeout()) {
            title_index = (title_index + 1) % titles.len;
            title.text = titles[title_index];
        }

        neko.scene.draw();
        neko.screen.present();
    }
}
