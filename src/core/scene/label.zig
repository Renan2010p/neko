//! `neko.scene.Label` — a node that draws text.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../base/types.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;
const gfx_sprite: type = @import("../graphics/sprite.zig");

pub const Label: type = struct {
    node: Node,
    text: []const u8,
    size: u32 = 24,
    color: types.Color = types.Color{ .r = 255, .g = 255, .b = 255 },
    center: bool = false,

    pub const Options: type = struct {
        text: []const u8,
        name: []const u8 = "label",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
        size: u32 = 24,
        color: types.Color = types.Color{ .r = 255, .g = 255, .b = 255 },
        center: bool = false,
    };

    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Label = try allocator.create(Label);
        self.* = Label{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
            .text = options.text,
            .size = options.size,
            .color = options.color,
            .center = options.center,
        };
        return &self.node;
    }

    /// Recovers the `Label` from its scene-tree node.
    pub fn from_node(n: *Node) *Label {
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
    const self: *Label = @ptrCast(@alignCast(ptr));
    gfx_sprite.text(self.text, at.x, at.y, gfx_sprite.Options{
        .size = self.size,
        .color = self.color,
        .center = self.center,
    });
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Label = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}
