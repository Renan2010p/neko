//! `neko.scene.Node2D` — a positioned container node.
//!
//! Like Godot's `Node2D`: it has a position (and, later, rotation/scale) and
//! offsets its whole subtree. It draws nothing itself.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../types.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;

pub const Node2D: type = struct {
    node: Node,

    pub const Options: type = struct {
        name: []const u8 = "",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Node2D = try allocator.create(Node2D);
        self.* = Node2D{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
        };
        return &self.node;
    }
};

const vtable: Node.VTable = Node.VTable{
    .ready = node_mod.no_ready,
    .process = node_mod.no_process,
    .draw = node_mod.no_draw,
    .input = node_mod.no_input,
    .deinit = deinit,
};

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Node2D = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}
