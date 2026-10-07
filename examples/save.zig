// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: save — binary serialization and save files.
//!
//! Up adds to the score, S writes a save file, L loads it back. Serialization
//! (`neko.save.Writer`/`Reader`) is pure; the file helpers go through the
//! backend, so on a platform without a filesystem they quietly no-op.
//!
//! Run it with:
//!
//!     zig build run-save

const std: type = @import("std");
const neko: type = @import("neko");

const SAVE_DIR: *const [13:0]u8 = ".neko_example";
const SAVE_FILE: *const [8:0]u8 = "demo.bin";

const Game: type = struct {
    score: i32 = 0,
    status: []const u8 = "up = +10 · s = save · l = load",

    pub fn start(self: *Game) void {
        self.load();
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| switch (key.key) {
                .escape => neko.lifecycle.request_stop(),
                .up => {
                    self.score += 10;
                    self.status = "score up";
                },
                .s => self.save(),
                .l => self.load(),
                else => {},
            },
            else => {},
        }
    }

    fn save(self: *Game) void {
        var writer: neko.save.Writer = neko.save.Writer.init(neko.allocator());
        defer writer.deinit();

        writer.int(self.score) catch {
            self.status = "encode failed";
            return;
        };
        writer.str_u32("neko") catch {
            self.status = "encode failed";
            return;
        };

        self.status = if (neko.save.write_file(SAVE_DIR, SAVE_FILE, writer.slice()))
            "saved"
        else
            "no filesystem (save skipped)";
    }

    fn load(self: *Game) void {
        const bytes: []u8 = neko.save.read_file(neko.allocator(), SAVE_DIR, SAVE_FILE, 4096) orelse {
            self.status = "no save file";
            return;
        };
        defer neko.allocator().free(bytes);

        var reader: neko.save.Reader = neko.save.Reader.init(bytes);
        self.score = reader.int() catch return;
        _ = reader.str_u32() catch return;
        self.status = "loaded";
    }

    pub fn draw(self: *Game) void {
        var buf: [96]u8 = undefined;
        const line: []const u8 = std.fmt.bufPrint(&buf, "score: {d}", .{self.score}) catch "";
        neko.text.draw(line, 320, 120, .{ .size = 40, .center = true, .color = neko.Color.hex(0x66ccff) });
        neko.text.draw(self.status, 320, 190, .{ .size = 20, .center = true });
        neko.text.draw("Escape to quit", 320, 320, .{ .size = 16, .center = true, .color = neko.Color.hex(0x8888aa) });
    }
};

pub fn main(init: std.process.Init) !void {
    var game: Game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — Save",
        .width = 640,
        .height = 360,
        .assets_dir = "examples/assets",
    });
}
