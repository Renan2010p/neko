//! `neko.scene.AnimatedSprite` — a node that plays a sequence of frames.
//!
//! Frames are sprite names (e.g. "hero_1", "hero_2", …). Switch animations by
//! calling `play` with a different frame list — a plain Zig `switch` picks it:
//!
//!     switch (state) {
//!         .idle => sprite.play(&IDLE, 6, true),
//!         .walk => sprite.play(&WALK, 10, true),
//!     }

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../base/types.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;
const gfx_sprite: type = @import("../graphics/sprite.zig");

pub const AnimatedSprite: type = struct {
    node: Node,
    frames: []const []const u8 = &.{},
    fps: f32 = 8,
    looping: bool = true,
    playing: bool = false,
    index: usize = 0,
    timer: f32 = 0,
    size: types.Point = types.Point{ .x = 64, .y = 64 },

    pub const Options: type = struct {
        frames: []const []const u8 = &.{},
        name: []const u8 = "animated_sprite",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
        size: types.Point = types.Point{ .x = 64, .y = 64 },
        fps: f32 = 8,
        looping: bool = true,
        playing: bool = true,
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *AnimatedSprite = try allocator.create(AnimatedSprite);
        self.* = AnimatedSprite{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
            .frames = options.frames,
            .fps = options.fps,
            .looping = options.looping,
            .playing = options.playing,
            .size = options.size,
        };
        return &self.node;
    }

    pub fn from_node(n: *Node) *AnimatedSprite {
        return @ptrCast(@alignCast(n));
    }

    /// Switches to another animation (frame list) and restarts it.
    pub fn play(self: *AnimatedSprite, frames: []const []const u8, fps: f32, looping: bool) void {
        self.frames = frames;
        self.fps = fps;
        self.looping = looping;
        self.index = 0;
        self.timer = 0;
        self.playing = true;
    }

    pub fn stop(self: *AnimatedSprite) void {
        self.playing = false;
    }

    /// The sprite name of the current frame.
    pub fn frame(self: *const AnimatedSprite) []const u8 {
        if (self.frames.len == 0) return "";
        return self.frames[self.index];
    }
};

const vtable: Node.VTable = Node.VTable{
    .ready = node_mod.no_ready,
    .process = process,
    .draw = draw,
    .input = node_mod.no_input,
    .deinit = deinit,
};

fn process(ptr: *anyopaque, dt: f32) void {
    const self: *AnimatedSprite = @ptrCast(@alignCast(ptr));
    if (!self.playing or self.frames.len == 0 or self.fps <= 0) return;

    const frame_dur: f32 = 1.0 / self.fps;
    self.timer += dt;
    while (self.timer >= frame_dur) {
        self.timer -= frame_dur;
        if (self.index + 1 < self.frames.len) {
            self.index += 1;
        } else if (self.looping) {
            self.index = 0;
        } else {
            self.playing = false;
            break;
        }
    }
}

fn draw(ptr: *anyopaque, at: types.Point) void {
    const self: *AnimatedSprite = @ptrCast(@alignCast(ptr));
    if (self.frames.len == 0) return;
    gfx_sprite.draw(self.frame(), at.x, at.y, self.size.x, self.size.y);
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *AnimatedSprite = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}
