//! `neko.scene.Node` — base of the scene tree.
//!
//! Inspired by Godot: a node has a name, a parent, children and a lifecycle
//! (`ready` once, then `process`/`draw` each frame, plus `input`). Concrete
//! nodes (Sprite, Label, Timer, …) embed a `Node` as their first field and
//! point at their own state through the vtable.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../base/types.zig");
const event: type = @import("../system/event.zig");
const log: type = @import("../system/log.zig");

/// A node in the scene tree.
pub const Node: type = struct {
    ptr: *anyopaque,
    vtable: *const VTable,
    name: []const u8 = "",
    parent: ?*Node = null,
    children: std.ArrayListUnmanaged(*Node) = .empty,
    allocator: Allocator,
    /// Local translation applied to this node and its subtree.
    pos: types.Point = types.Point{ .x = 0, .y = 0 },
    /// When false, this node and its subtree are not drawn (they still update).
    visible: bool = true,
    ready_done: bool = false,

    pub const VTable: type = struct {
        ready: *const fn (ptr: *anyopaque) void,
        process: *const fn (ptr: *anyopaque, dt: f32) void,
        draw: *const fn (ptr: *anyopaque, at: types.Point) void,
        input: *const fn (ptr: *anyopaque, ev: event.Event) void,
        deinit: *const fn (ptr: *anyopaque, allocator: Allocator) void,
    };

    pub const Options: type = struct {
        name: []const u8 = "",
        pos: types.Point = types.Point{ .x = 0, .y = 0 },
    };

    /// A plain container node (no drawing of its own).
    pub fn create(allocator: Allocator, options: Options) !*Node {
        const self: *Node = try allocator.create(Node);
        log.debug("scene: new node '{s}' ({d} bytes)", .{ options.name, @sizeOf(Node) });
        self.* = Node{
            .ptr = @ptrCast(self),
            .vtable = &empty_vtable,
            .allocator = allocator,
            .name = options.name,
            .pos = options.pos,
        };
        return self;
    }

    /// Attaches `child` under this node.
    pub fn add(self: *Node, child: *Node) void {
        child.parent = self;
        self.children.append(self.allocator, child) catch {};
    }

    /// The direct child named `child_name`, or null.
    pub fn getChild(self: *Node, child_name: []const u8) ?*Node {
        for (self.children.items) |c| {
            if (std.mem.eql(u8, c.name, child_name)) return c;
        }
        return null;
    }

    /// Depth-first search for a node named `name` anywhere under this one.
    pub fn find(self: *Node, search_name: []const u8) ?*Node {
        if (std.mem.eql(u8, self.name, search_name)) return self;
        for (self.children.items) |c| {
            if (c.find(search_name)) |found| return found;
        }
        return null;
    }

    /// Resolves a Godot-style path relative to this node:
    /// `"Hud/Score"` (descend), `"."` (self) and `".."` (parent) are supported.
    /// Returns null if any segment is missing.
    pub fn get_node(self: *Node, path: []const u8) ?*Node {
        var current: *Node = self;
        var rest: []const u8 = path;

        while (rest.len > 0) {
            const slash: ?usize = std.mem.indexOfScalar(u8, rest, '/');
            const segment: []const u8 = if (slash) |i| rest[0..i] else rest;

            if (segment.len > 0) {
                if (std.mem.eql(u8, segment, ".")) {
                    // stay on the current node
                } else if (std.mem.eql(u8, segment, "..")) {
                    current = current.parent orelse return null;
                } else {
                    current = current.getChild(segment) orelse return null;
                }
            }

            if (slash) |i| {
                rest = rest[i + 1 ..];
            } else break;
        }

        return current;
    }

    /// Frees and detaches every child.
    pub fn clearChildren(self: *Node) void {
        for (self.children.items) |child| child.deinit();
        self.children.clearRetainingCapacity();
    }

    pub fn process(self: *Node, dt: f32) void {
        if (!self.ready_done) {
            self.ready_done = true;
            self.vtable.ready(self.ptr);
        }
        self.vtable.process(self.ptr, dt);
        for (self.children.items) |child| child.process(dt);
    }

    pub fn draw(self: *Node, at: types.Point) void {
        if (!self.visible) return;
        const world: types.Point = types.Point{ .x = at.x + self.pos.x, .y = at.y + self.pos.y };
        self.vtable.draw(self.ptr, world);
        for (self.children.items) |child| child.draw(world);
    }

    pub fn input(self: *Node, ev: event.Event) void {
        if (!self.visible) return;
        self.vtable.input(self.ptr, ev);
        for (self.children.items) |child| child.input(ev);
    }

    /// Frees this node and its whole subtree.
    pub fn deinit(self: *Node) void {
        for (self.children.items) |child| child.deinit();
        self.children.deinit(self.allocator);
        self.vtable.deinit(self.ptr, self.allocator);
    }
};

/// No-op lifecycle hooks, for nodes that don't need them.
pub fn no_ready(_: *anyopaque) void {}
pub fn no_process(_: *anyopaque, _: f32) void {}
pub fn no_draw(_: *anyopaque, _: types.Point) void {}
pub fn no_input(_: *anyopaque, _: event.Event) void {}

/// Default deinit for a node whose tree cell *is* the allocation.
pub fn no_deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Node = @ptrCast(@alignCast(ptr));
    allocator.destroy(self);
}

const empty_vtable: Node.VTable = Node.VTable{
    .ready = no_ready,
    .process = no_process,
    .draw = no_draw,
    .input = no_input,
    .deinit = no_deinit,
};
