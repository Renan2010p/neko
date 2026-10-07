//! `neko.scene.Timer` — a countdown node.
//!
//! Like Godot's `Timer`: it counts down and sets `timeout` true. Poll
//! `is_timeout()` (or read `timeout`) and call `start()` again, or let a
//! repeating timer auto-restart.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;

pub const Timer: type = struct {
    node: Node,
    wait_time: f32 = 1.0,
    time_left: f32 = 0,
    one_shot: bool = true,
    running: bool = false,
    timeout: bool = false,

    pub const Options: type = struct {
        name: []const u8 = "timer",
        wait_time: f32 = 1.0,
        one_shot: bool = true,
        autostart: bool = true,
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Timer = try allocator.create(Timer);
        self.* = Timer{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
            },
            .wait_time = options.wait_time,
            .one_shot = options.one_shot,
            .running = options.autostart,
            .time_left = options.wait_time,
        };
        return &self.node;
    }

    /// (Re)starts the countdown.
    pub fn start(self: *Timer) void {
        self.time_left = self.wait_time;
        self.running = true;
        self.timeout = false;
    }

    pub fn stop(self: *Timer) void {
        self.running = false;
    }

    /// True on the frame the countdown reached zero.
    pub fn is_timeout(self: *const Timer) bool {
        return self.timeout;
    }
};

const vtable: Node.VTable = Node.VTable{
    .ready = node_mod.no_ready,
    .process = process,
    .draw = node_mod.no_draw,
    .input = node_mod.no_input,
    .deinit = deinit,
};

fn process(ptr: *anyopaque, dt: f32) void {
    const self: *Timer = @ptrCast(@alignCast(ptr));
    self.timeout = false;
    if (!self.running) return;

    self.time_left -= dt;
    if (self.time_left <= 0) {
        self.timeout = true;
        if (self.one_shot) {
            self.running = false;
        } else {
            self.time_left = self.wait_time;
        }
    }
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Timer = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Timer counts down and fires once" {
    const node: *Node = try Timer.create(testing.allocator, .{
        .wait_time = 0.1,
        .one_shot = true,
        .autostart = true,
    });
    defer node.deinit();

    const timer: *Timer = @ptrCast(@alignCast(node));
    node.process(0.05);
    try testing.expect(!timer.is_timeout());

    node.process(0.06);
    try testing.expect(timer.is_timeout());

    node.process(0.1);
    try testing.expect(!timer.is_timeout());
    try testing.expect(!timer.running);
}
