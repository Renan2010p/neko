//! `neko.scene.Script` — attach a plain object to the tree as a "script".
//!
//! In Godot you attach a script to a node; in Neko you attach a value. Any
//! struct with **optional, public** methods can become a node:
//!
//! ```zig
//! const Player = struct {
//!     x: f32 = 0,
//!
//!     pub fn ready(self: *Player) void { /* once */ }
//!     pub fn process(self: *Player, dt: f32) void { self.x += 60 * dt; }
//!     pub fn draw(self: *Player, at: neko.Point) void { /* draw at world pos */ }
//!     pub fn input(self: *Player, ev: neko.Event) void { /* handle event */ }
//! };
//!
//! var player = Player{};
//! _ = try neko.scene.addScript(root, &player, .{ .name = "Player" });
//! ```
//!
//! The methods must be `pub`. The object must outlive the node (a field of the
//! game, or a value on the stack in `main`) — the node only stores a pointer to
//! it, never copies it.
//!
//! Use `Script` directly with explicit `Hooks` when you need a callback without
//! a backing type.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../types.zig");
const event: type = @import("../system/event.zig");
const node_mod: type = @import("node.zig");
const Node: type = node_mod.Node;

/// A node that forwards the lifecycle to a user object or to explicit hooks.
pub const Script: type = struct {
    node: Node,
    userdata: ?*anyopaque = null,
    hooks: Hooks,

    /// The function pointers a script can provide. All optional; a null one is
    /// simply skipped. Generated at comptime by `add` for a typed object, or
    /// filled by hand when using `create`.
    pub const Hooks: type = struct {
        ready: ?*const fn (ctx: *anyopaque) void = null,
        process: ?*const fn (ctx: *anyopaque, dt: f32) void = null,
        draw: ?*const fn (ctx: *anyopaque, at: types.Point) void = null,
        input: ?*const fn (ctx: *anyopaque, ev: event.Event) void = null,
        deinit: ?*const fn (ctx: *anyopaque) void = null,
    };

    pub const Options: type = struct {
        name: []const u8 = "script",
        pos: types.Point = .{ .x = 0, .y = 0 },
    };

    /// Creates a script node from explicit hooks. Prefer `add` when you have an
    /// object with methods.
    pub fn create(allocator: Allocator, hooks: Hooks, userdata: ?*anyopaque, options: Options) !*Node {
        const self: *Script = try allocator.create(Script);
        self.* = Script{
            .node = Node{
                .ptr = @ptrCast(self),
                .vtable = &vtable,
                .allocator = allocator,
                .name = options.name,
                .pos = options.pos,
            },
            .userdata = userdata,
            .hooks = hooks,
        };
        return &self.node;
    }

    /// Attaches `object` under `parent` and returns the script node.
    pub fn add(parent: *Node, object: anytype, options: Options) !*Script {
        const T: type = @TypeOf(object.*);
        const data: *anyopaque = @ptrCast(object);
        const node: *Node = try create(parent.allocator, comptime hooksFor(T), data, options);
        parent.add(node);
        return from_node(node);
    }

    /// Recovers the `Script` from its scene-tree node.
    pub fn from_node(n: *Node) *Script {
        return @ptrCast(@alignCast(n));
    }
};

/// Builds the hook table for a type, wiring only the methods it declares.
fn hooksFor(comptime T: type) Script.Hooks {
    return .{
        .ready = if (@hasDecl(T, "ready")) struct {
            fn call(ctx: *anyopaque) void {
                @as(*T, @ptrCast(@alignCast(ctx))).ready();
            }
        }.call else null,
        .process = if (@hasDecl(T, "process")) struct {
            fn call(ctx: *anyopaque, dt: f32) void {
                @as(*T, @ptrCast(@alignCast(ctx))).process(dt);
            }
        }.call else null,
        .draw = if (@hasDecl(T, "draw")) struct {
            fn call(ctx: *anyopaque, at: types.Point) void {
                @as(*T, @ptrCast(@alignCast(ctx))).draw(at);
            }
        }.call else null,
        .input = if (@hasDecl(T, "input")) struct {
            fn call(ctx: *anyopaque, ev: event.Event) void {
                @as(*T, @ptrCast(@alignCast(ctx))).input(ev);
            }
        }.call else null,
        .deinit = if (@hasDecl(T, "deinit")) struct {
            fn call(ctx: *anyopaque) void {
                @as(*T, @ptrCast(@alignCast(ctx))).deinit();
            }
        }.call else null,
    };
}

const vtable: Node.VTable = .{
    .ready = ready,
    .process = process,
    .draw = draw,
    .input = input,
    .deinit = deinit,
};

fn ready(ptr: *anyopaque) void {
    const self: *Script = @ptrCast(@alignCast(ptr));
    const call: *const fn (ctx: *anyopaque) void = self.hooks.ready orelse return;
    call(self.userdata orelse return);
}

fn process(ptr: *anyopaque, dt: f32) void {
    const self: *Script = @ptrCast(@alignCast(ptr));
    const call: *const fn (ctx: *anyopaque, dt: f32) void = self.hooks.process orelse return;
    call(self.userdata orelse return, dt);
}

fn draw(ptr: *anyopaque, at: types.Point) void {
    const self: *Script = @ptrCast(@alignCast(ptr));
    const call: *const fn (ctx: *anyopaque, at: types.Point) void = self.hooks.draw orelse return;
    call(self.userdata orelse return, at);
}

fn input(ptr: *anyopaque, ev: event.Event) void {
    const self: *Script = @ptrCast(@alignCast(ptr));
    const call: *const fn (ctx: *anyopaque, ev: event.Event) void = self.hooks.input orelse return;
    call(self.userdata orelse return, ev);
}

fn deinit(ptr: *anyopaque, allocator: Allocator) void {
    const self: *Script = @ptrCast(@alignCast(ptr));
    if (self.hooks.deinit) |call| {
        if (self.userdata) |data| call(data);
    }
    allocator.destroy(self);
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "script node dispatches to the object's methods" {
    const Counter: type = struct {
        ready_count: u32 = 0,
        process_total: f32 = 0,

        pub fn ready(self: *@This()) void {
            self.ready_count += 1;
        }
        pub fn process(self: *@This(), dt: f32) void {
            self.process_total += dt;
        }
    };

    var counter: Counter = Counter{};
    const root: *Node = try Node.create(testing.allocator, .{ .name = "root" });
    defer root.deinit();

    _ = try Script.add(root, &counter, .{ .name = "counter" });

    root.process(0.25);
    root.process(0.25);

    try testing.expectEqual(@as(u32, 1), counter.ready_count); // ready only once
    try testing.expectApproxEqAbs(@as(f32, 0.5), counter.process_total, 0.0001);
}
