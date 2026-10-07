//! `neko.scene` — the scene tree (Godot-inspired).
//!
//! A scene is a `Node` tree. The manager keeps a stack of scene roots and
//! forwards the frame to the top one:
//!
//!     neko.scene.init(allocator);
//!     const root = try neko.scene.Node.create(allocator, .{ .name = "menu" });
//!     _ = root.add(try neko.scene.Sprite.create(allocator, .{ .image = "logo" }));
//!     neko.scene.switch_to(root);
//!     ...
//!     neko.scene.process(dt);
//!     neko.scene.draw();

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("../base/types.zig");
const event: type = @import("../system/event.zig");
const draw_mod: type = @import("../graphics/draw.zig");

const node_mod: type = @import("node.zig");
const node2d_mod: type = @import("node2d.zig");
const sprite_mod: type = @import("sprite.zig");
const label_mod: type = @import("label.zig");
const timer_mod: type = @import("timer.zig");
const animated_sprite_mod: type = @import("animated_sprite.zig");
const script_mod: type = @import("script.zig");

pub const Node: type = node_mod.Node;
pub const Node2D: type = node2d_mod.Node2D;
pub const Sprite: type = sprite_mod.Sprite;
pub const Label: type = label_mod.Label;
pub const Timer: type = timer_mod.Timer;
pub const AnimatedSprite: type = animated_sprite_mod.AnimatedSprite;
pub const Script: type = script_mod.Script;

/// A **scene** is a root `Node` (plus its children). `neko.scene.create`
/// returns one; `neko.window.switch_to` makes it the current scene. This alias
/// exists so `*neko.Scene` reads the way you think about it.
///
/// Switching scenes frees the previous root (and its whole subtree), exactly
/// like Godot's `change_scene`: to return to a scene, build it again.
pub const Scene: type = Node;

// Godot-style names (with 2D/3D suffix) — the same types, exposed for the
// future 2D/3D split. `Node3D`/`Sprite3D`/… land here when 3D arrives.
pub const Sprite2D: type = sprite_mod.Sprite;
pub const Label2D: type = label_mod.Label;
pub const AnimatedSprite2D: type = animated_sprite_mod.AnimatedSprite;

// No-op lifecycle hooks, handy when a node only needs one or two.
pub const no_ready: *const fn (*anyopaque) void = node_mod.no_ready;
pub const no_process: *const fn (*anyopaque, f32) void = node_mod.no_process;
pub const no_draw: *const fn (*anyopaque, types.Point) void = node_mod.no_draw;
pub const no_input: *const fn (*anyopaque, event.Event) void = node_mod.no_input;

// ── Builders ─────────────────────────────────────────────────────────────
// Create a child node and return the concrete type, so you can tweak it:
//     const title = try neko.scene.addLabel(root, .{ .text = "Hi" });

pub fn addLabel(parent: *Node, options: Label.Options) !*Label {
    const n: *Node = try Label.create(parent.allocator, options);
    parent.add(n);
    return Label.from_node(n);
}

pub fn addSprite(parent: *Node, options: Sprite.Options) !*Sprite {
    const n: *Node = try Sprite.create(parent.allocator, options);
    parent.add(n);
    return Sprite.from_node(n);
}

pub fn addNode2D(parent: *Node, options: Node2D.Options) !*Node2D {
    const n: *Node = try Node2D.create(parent.allocator, options);
    parent.add(n);
    return @ptrCast(@alignCast(n));
}

pub fn addTimer(parent: *Node, options: Timer.Options) !*Timer {
    const n: *Node = try Timer.create(parent.allocator, options);
    parent.add(n);
    return @ptrCast(@alignCast(n));
}

pub fn addAnimatedSprite(parent: *Node, options: AnimatedSprite.Options) !*AnimatedSprite {
    const n: *Node = try AnimatedSprite.create(parent.allocator, options);
    parent.add(n);
    return AnimatedSprite.from_node(n);
}

/// Attaches `object` as a script node. `object` is any pointer to a struct with
/// optional, public `ready`/`process`/`draw`/`input` methods. See
/// `neko.scene.Script`.
pub fn addScript(parent: *Node, object: anytype, options: Script.Options) !*Script {
    return Script.add(parent, object, options);
}

var stack: std.ArrayListUnmanaged(*Node) = .empty;
var allocator: Allocator = undefined;
var clear_color: types.Color = types.Color{ .r = 0, .g = 0, .b = 0 };

/// Prepares the manager with the allocator that owns the scenes.
pub fn init(alloc: Allocator) void {
    allocator = alloc;
}

/// Creates a scene root node. Short for `Node.create`.
pub fn create(allocator_: Allocator, options: Node.Options) !*Node {
    return Node.create(allocator_, options);
}

/// Frees every scene on the stack.
pub fn deinit() void {
    for (stack.items) |root| root.deinit();
    stack.deinit(allocator);
    stack = .empty;
}

/// Replaces the top scene (or pushes if the stack was empty), freeing the old.
pub fn switch_to(root: *Node) void {
    if (stack.items.len > 0) {
        stack.items[stack.items.len - 1].deinit();
        stack.items[stack.items.len - 1] = root;
    } else {
        stack.append(allocator, root) catch root.deinit();
    }
}

/// Pushes a scene on top, keeping the current one alive underneath.
pub fn push(root: *Node) void {
    stack.append(allocator, root) catch root.deinit();
}

/// Pops the top scene, freeing it. Returns false if the stack was empty.
pub fn pop() bool {
    if (stack.pop()) |root| {
        root.deinit();
        return true;
    }
    return false;
}

pub fn process(dt: f32) void {
    if (current()) |root| root.process(dt);
}

/// Clears the screen with the current clear color, then draws the scene.
pub fn draw() void {
    draw_mod.clear(clear_color);
    if (current()) |root| root.draw(.{ .x = 0, .y = 0 });
}

/// Sets the color `draw()` clears the screen with (default black).
pub fn set_clear_color(color: types.Color) void {
    clear_color = color;
}

pub fn input(ev: event.Event) void {
    if (current()) |root| root.input(ev);
}

/// True while at least one scene is on the stack.
pub fn active() bool {
    return stack.items.len > 0;
}

/// The current scene root, or null.
pub fn current() ?*Node {
    if (stack.items.len > 0) return stack.items[stack.items.len - 1];
    return null;
}

/// Resolves a node path from the current scene (e.g. `"Hud/Score"`, `"./Hud"`).
pub fn get_node(path: []const u8) ?*Node {
    if (current()) |root| return root.get_node(path);
    return null;
}

/// Finds a node by name anywhere in the current scene (depth-first).
pub fn find(search_name: []const u8) ?*Node {
    if (current()) |root| return root.find(search_name);
    return null;
}

/// The number of scenes on the stack.
pub fn depth() usize {
    return stack.items.len;
}

/// The name of the current scene, or "".
pub fn name() []const u8 {
    if (current()) |root| return root.name;
    return "";
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "scene manager switches scenes and frees the old one" {
    init(testing.allocator);
    defer deinit();

    const menu: *Node = try create(testing.allocator, .{ .name = "menu" });
    const play: *Node = try create(testing.allocator, .{ .name = "play" });

    try testing.expect(!active());
    switch_to(menu);
    try testing.expect(active());
    try testing.expectEqual(@as(usize, 1), depth());
    try testing.expectEqualStrings("menu", current().?.name);

    // switch_to replaces the top and frees the previous scene (menu).
    switch_to(play);
    try testing.expectEqual(@as(usize, 1), depth());
    try testing.expectEqualStrings("play", current().?.name);

    // push/pop keep the scene underneath alive.
    const pause: *Node = try create(testing.allocator, .{ .name = "pause" });
    push(pause);
    try testing.expectEqual(@as(usize, 2), depth());
    try testing.expectEqualStrings("pause", name());
    try testing.expect(pop());
    try testing.expectEqualStrings("play", current().?.name);

    _ = pop();
    try testing.expect(!active());
}
