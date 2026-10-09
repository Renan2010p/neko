// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.scene.Animator` — named animation clips over sprite frames.
//!
//! Generalizes `AnimatedSprite`: instead of switching a raw frame list by hand,
//! you register named clips once and call `play("run")`:
//!
//! ```zig
//! const anim = try neko.scene.addAnimator(root, .{
//!     .clips = &.{
//!         .{ .name = "idle", .frames = &.{ "hero_0", "hero_1" }, .fps = 4 },
//!         .{ .name = "run",  .frames = &.{ "hero_2", "hero_3" }, .fps = 10 },
//!     },
//!     .play = "idle",
//! });
//! anim.play("run");
//! ```

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../base/types.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;
const gfx_sprite: type = @import("../graphics/sprite.zig");

/// One named animation: a list of sprite frame names.
pub const Clip: type = struct {
    name: []const u8,
    frames: []const []const u8,
    fps: f32 = 8,
    looping: bool = true,
};

pub const Animator: type = struct {
    node: Node,
    clips: []const Clip = &.{},
    clip: usize = 0,
    frame: usize = 0,
    timer: f32 = 0,
    playing: bool = true,
    finished: bool = false,
    size: types.Point = types.Point{ .x = 64, .y = 64 },

    pub const Options: type = struct {
        name: []const u8 = "animator",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
        size: types.Point = types.Point{ .x = 64, .y = 64 },
        clips: []const Clip = &.{},
        /// Clip to start on (empty = the first).
        play: []const u8 = "",
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Animator = try allocator.create(Animator);
        self.* = Animator{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
            .clips = options.clips,
            .size = options.size,
        };
        if (options.play.len > 0) self.play(options.play);
        return &self.node;
    }

    pub fn from_node(n: *Node) *Animator {
        return @ptrCast(@alignCast(n));
    }

    /// Switches to the clip named `name` and restarts it. No-op if unknown.
    pub fn play(self: *Animator, name: []const u8) void {
        const index: usize = self.findClip(name) orelse return;
        self.clip = index;
        self.frame = 0;
        self.timer = 0;
        self.playing = true;
        self.finished = false;
    }

    pub fn stop(self: *Animator) void {
        self.playing = false;
    }

    /// The sprite frame name currently shown.
    pub fn currentFrame(self: *const Animator) []const u8 {
        if (self.clips.len == 0) return "";
        const clip: Clip = self.clips[self.clip];
        if (clip.frames.len == 0) return "";
        return clip.frames[self.frame];
    }

    fn findClip(self: *const Animator, name: []const u8) ?usize {
        for (self.clips, 0..) |clip, i| {
            if (std.mem.eql(u8, clip.name, name)) return i;
        }
        return null;
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
    const self: *Animator = @ptrCast(@alignCast(ptr));
    if (!self.playing or self.clips.len == 0) return;
    const clip: Clip = self.clips[self.clip];
    if (clip.frames.len == 0 or clip.fps <= 0) return;

    const frame_dur: f32 = 1.0 / clip.fps;
    self.timer += dt;
    while (self.timer >= frame_dur) {
        self.timer -= frame_dur;
        if (self.frame + 1 < clip.frames.len) {
            self.frame += 1;
        } else if (clip.looping) {
            self.frame = 0;
        } else {
            self.playing = false;
            self.finished = true;
            break;
        }
    }
}

fn draw(ptr: *anyopaque, at: types.Point) void {
    const self: *Animator = @ptrCast(@alignCast(ptr));
    if (self.clips.len == 0) return;
    if (self.clips[self.clip].frames.len == 0) return;
    gfx_sprite.draw(self.currentFrame(), at.x, at.y, self.size.x, self.size.y);
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Animator = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Animator plays a named clip and advances frames" {
    const clips: [1]Clip = .{.{
        .name = "run",
        .frames = &.{ "a", "b", "c" },
        .fps = 10,
        .looping = true,
    }};

    const node: *Node = try Animator.create(testing.allocator, .{ .clips = &clips, .play = "run" });
    defer node.deinit();

    const anim: *Animator = Animator.from_node(node);
    try testing.expectEqualStrings("a", anim.currentFrame());

    node.process(0.11); // one frame
    try testing.expectEqualStrings("b", anim.currentFrame());

    node.process(0.1);
    try testing.expectEqualStrings("c", anim.currentFrame());

    node.process(0.1); // loops back
    try testing.expectEqualStrings("a", anim.currentFrame());
}
