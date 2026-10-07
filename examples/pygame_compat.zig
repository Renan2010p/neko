// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Example: pygame compat — draw with a pygame-shaped API on top of Neko.
//!
//! A `Surface` sprite is generated procedurally, then shapes, alpha blits and
//! a flipped copy are drawn to a 512x288 canvas and presented scaled up.
//!
//! Run it with:
//!
//!     zig build run-pygame_compat

const std: type = @import("std");
const neko: type = @import("neko");
const pg: type = @import("neko_pygame");

const Game: type = struct {
    hero: pg.Surface = undefined,
    hero_flip: ?pg.Surface = null,
    elapsed: f32 = 0,

    pub fn start(self: *Game) void {
        // build a little heroine sprite the way Vesper's art.py does
        self.hero = pg.Surface.init(std.heap.c_allocator, 16, 22) catch @panic("oom");
        pg.draw.rect(&self.hero, pg.Rect.init(5, 4, 6, 12), pg.Color.hex(0xc43a5c), true);
        pg.draw.circle(&self.hero, 8, 5, 3, pg.Color.hex(0xf5c462), true);
        pg.draw.circle(&self.hero, 11, 5, 1, pg.Color.hex(0x60f0ff), true);
        pg.draw.rect(&self.hero, pg.Rect.init(6, 16, 4, 5), pg.Color.hex(0x424460), true);
    }

    pub fn update(self: *Game, dt: f32) void {
        self.elapsed += dt;
    }

    pub fn draw(self: *Game) void {
        const s: *pg.Surface = pg.surface();

        s.fill(pg.Color.hex(0x0a0c16));
        pg.draw.rect(s, pg.Rect.init(0, 180, 512, 108), pg.Color.hex(0x181226), true);

        // ellipse (translucent), polygon, circles, lines
        pg.draw.ellipse(s, pg.Rect.init(40, 88, 120, 72), pg.Color.rgba(90, 200, 255, 150), true);
        pg.draw.polygon(s, &.{
            pg.Point.init(300, 56),  pg.Point.init(432, 118),
            pg.Point.init(360, 196), pg.Point.init(258, 150),
        }, pg.Color.hex(0xf0c040));
        pg.draw.circle(s, 150, 224, 26, pg.Color.hex(0x4fd97a), true);
        pg.draw.circle(s, 150, 224, 26, pg.Color.white, false);
        pg.draw.line(s, 20, 24, 492, 24, pg.Color.hex(0x5a6a8a));
        pg.draw.lines(s, &.{
            pg.Point.init(20, 262), pg.Point.init(120, 232), pg.Point.init(200, 266),
        }, pg.Color.hex(0xff6a4a));

        // the procedural sprite, bobbing, plus a flipped copy
        const bob: i32 = @intFromFloat(@sin(self.elapsed * 2.0) * 4.0);
        s.blit(&self.hero, pg.Point.init(64, 196 + bob));
        const flipped: *const pg.Surface = pg.transform.maybeFlipX(&self.hero, true, &self.hero_flip);
        s.blit(flipped, pg.Point.init(104, 196 + bob));

        pg.flip();
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .quit => neko.lifecycle.request_stop(),
            .key_down => |key| if (key.key == .escape) neko.lifecycle.request_stop(),
            else => {},
        }
    }
};

pub fn main(init: std.process.Init) !void {
    _ = pg.set_mode(512, 288); // the virtual canvas
    var game: Game = Game{};
    game.start();
    try neko.app.runOptions(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "Neko — pygame compat",
        .width = 1024,
        .height = 576,
    }, .{ .clear_color = null, .load_default_font = false });
}
