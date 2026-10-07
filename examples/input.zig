// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: input — keyboard, mouse and window events.
//!
//! Shows the raw key code and a human-readable key name, the cursor position,
//! click count and wheel delta. Escape quits.
//!
//! Run it with:
//!
//!     zig build run-input

const std: type = @import("std");
const neko: type = @import("neko");

const Game: type = struct {
    last_name: []const u8 = "-",
    last_code: i32 = 0,
    mouse: neko.Point = .{},
    clicks: u32 = 0,
    wheel: f32 = 0,

    pub fn event(self: *Game, ev: neko.Event) void {
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| {
                self.last_name = key.name;
                self.last_code = key.code;
                if (key.key == .escape) neko.lifecycle.request_stop();
            },
            .mouse_motion => |motion| self.mouse = .{ .x = motion.x, .y = motion.y },
            .mouse_button_down => self.clicks += 1,
            .mouse_wheel => |wheel| self.wheel += wheel.y,
            else => {},
        }
    }

    pub fn draw(self: *Game) void {
        var buf: [128]u8 = undefined;

        const key_line: []const u8 = std.fmt.bufPrint(&buf, "last key:  {s}  (code {d})", .{ self.last_name, self.last_code }) catch "";
        neko.text.draw(key_line, 24, 40, .{ .size = 22 });

        var buf2: [128]u8 = undefined;
        const mouse_line: []const u8 = std.fmt.bufPrint(&buf2, "mouse:     {d}, {d}", .{ self.mouse.x, self.mouse.y }) catch "";
        neko.text.draw(mouse_line, 24, 80, .{ .size = 22 });

        var buf3: [128]u8 = undefined;
        const click_line: []const u8 = std.fmt.bufPrint(&buf3, "clicks:    {d}", .{self.clicks}) catch "";
        neko.text.draw(click_line, 24, 120, .{ .size = 22 });

        var buf4: [128]u8 = undefined;
        const wheel_line: []const u8 = std.fmt.bufPrint(&buf4, "wheel:     {d:.1}", .{self.wheel}) catch "";
        neko.text.draw(wheel_line, 24, 160, .{ .size = 22 });

        // A cursor marker follows the mouse.
        neko.draw.circle(self.mouse.x, self.mouse.y, 6, neko.Color.hex(0x66ccff), true);

        neko.text.draw("Escape to quit", 24, 320, .{ .size = 16, .color = neko.Color.hex(0x8888aa) });
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Input",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
