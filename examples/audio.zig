// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: audio — load and play a sound.
//!
//! Space plays the sound once, Enter loops it, and M/S change the master
//! volume. A missing `examples/assets/blip.wav` simply yields no sound
//! (`neko.sound.load` returns null), so the example still runs.
//!
//! Run it with:
//!
//!     zig build run-audio

const std: type = @import("std");
const neko: type = @import("neko");

const Game: type = struct {
    sound: ?neko.SoundHandle = null,
    master: i32 = 80,
    last: []const u8 = "space = play · enter = loop · m/s = volume",

    pub fn start(self: *Game) void {
        self.sound = neko.sound.load("examples/assets/blip.wav");
        neko.sound.master_volume(self.master);
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| switch (key.key) {
                .escape => neko.lifecycle.request_stop(),
                .space => if (self.sound) |s| {
                    _ = neko.sound.play(s, 0, -1);
                    self.last = "played once";
                },
                .enter => if (self.sound) |s| {
                    _ = neko.sound.play(s, -1, -1);
                    self.last = "looping";
                },
                .m => {
                    self.master = @min(100, self.master + 10);
                    neko.sound.master_volume(self.master);
                    self.last = "volume up";
                },
                .s => {
                    self.master = @max(0, self.master - 10);
                    neko.sound.master_volume(self.master);
                    self.last = "volume down";
                },
                else => {},
            },
            else => {},
        }
    }

    pub fn draw(self: *Game) void {
        neko.text.draw("Audio", 320, 60, .{ .size = 40, .center = true, .color = neko.Color.hex(0x66ccff) });
        neko.text.draw(self.last, 320, 150, .{ .size = 20, .center = true });
        neko.text.draw(
            if (self.sound == null) "no sound loaded (asset missing)" else "sound ready",
            320,
            190,
            .{ .size = 18, .center = true, .color = neko.Color.hex(0x8888aa) },
        );

        var buf: [64]u8 = undefined;
        const line: []const u8 = std.fmt.bufPrint(&buf, "master volume: {d}", .{self.master}) catch "";
        neko.text.draw(line, 320, 250, .{ .size = 20, .center = true });
        neko.text.draw("Escape to quit", 320, 320, .{ .size = 16, .center = true, .color = neko.Color.hex(0x8888aa) });
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Audio",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
