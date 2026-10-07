//! `neko.scene.Sprite` — a node that draws a named sprite.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../types.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;
const gfx_sprite: type = @import("../graphics/sprite.zig");

pub const Sprite: type = struct {
    node: Node,
    image: []const u8,
    size: types.Point = types.Point{ .x = 64, .y = 64 },

    pub const Options: type = struct {
        image: []const u8,
        name: []const u8 = "sprite",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
        size: types.Point = types.Point{ .x = 64, .y = 64 },
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Sprite = try allocator.create(Sprite);
        self.* = Sprite{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
            .image = options.image,
            .size = options.size,
        };
        return &self.node;
    }

    /// Recovers the `Sprite` from its scene-tree node.
    pub fn from_node(n: *Node) *Sprite {
        return @ptrCast(@alignCast(n));
    }
};

const vtable: Node.VTable = Node.VTable{
    .ready = node_mod.no_ready,
    .process = node_mod.no_process,
    .draw = draw,
    .input = node_mod.no_input,
    .deinit = deinit,
};

fn draw(ptr: *anyopaque, at: types.Point) void {
    const self: *Sprite = @ptrCast(@alignCast(ptr));
    gfx_sprite.draw(self.image, at.x, at.y, self.size.x, self.size.y);
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Sprite = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}
