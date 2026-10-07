# Neko for Godot users (code-first)

Neko borrows Godot's **runtime architecture** — a window, a tree of nodes, and
switchable scenes — but there is **no editor and no `.tscn` files**. Everything
is code:

- a **node** is a `Node` value (or your own struct);
- a **scene** is a function/type that builds a subtree;
- a **script** is a plain struct with `ready`/`process`/`draw`/`input` methods.

If you know Godot's `Node`, `_process`, `change_scene_to` and `$Path`, the
mapping below is the fastest way in.

## Concept map

| Godot | Neko | Notes |
|-------|------|-------|
| `Window` / viewport | `neko.window.Window` | one per process |
| `SceneTree` | `neko.scene` | module-level manager |
| `change_scene_to*` | `window.switch_to(scene)` | frees the previous scene |
| `add_child` / `remove_child` | `node.add(child)` / `node.clearChildren()` | |
| `_ready()` | `pub fn ready(self)` on a script | once, on first process |
| `_process(delta)` | `pub fn process(self, dt)` | every frame |
| `_draw()` | `pub fn draw(self, at)` | `at` is the world offset |
| `_input(event)` | `pub fn input(self, ev)` | every event |
| `_exit_tree` / free | `pub fn deinit(self)` | when the node is freed |
| `Node2D` | `neko.scene.Node2D` | positioned container |
| `Sprite2D` | `neko.scene.Sprite` / `Sprite2D` | named sprite |
| `Label` | `neko.scene.Label` / `Label2D` | text |
| `Timer` | `neko.scene.Timer` | `is_timeout()` |
| `AnimatedSprite2D` | `neko.scene.AnimatedSprite` | `play(frames, fps, loop)` |
| `$Path/To/Node` | `neko.scene.get_node("Path/To/Node")` | `.`, `..`, `/` supported |
| `find_child` / groups | `neko.scene.find("Name")` | depth-first by name |
| Script attached to a node | `neko.scene.addScript(parent, &obj, .{})` | your struct is the script |
| Preload / PackedScene | a function returning `*neko.Scene` | code-first |
| Autoload / singleton | a module-level `var` | plain Zig |
| `queue_free` | — | planned |
| Signals (`connect`/`emit`) | — | planned |
| 3D (`Node3D`, `Mesh`, `Camera3D`) | — | planned (needs a 3D backend) |
| Physics (`Area2D`, bodies) | — | planned |
| `Control` / UI | — | planned |

## A scene + script, side by side

In Godot you would have a scene file plus a script. In Neko:

```zig
const std = @import("std");
const neko = @import("neko");

// The "script": gameplay lives in a struct.
const Player = struct {
    x: f32 = 300,
    speed: f32 = 180,

    pub fn ready(self: *Player) void { _ = self; } // once
    pub fn process(self: *Player, dt: f32) void {
        if (neko.input.key(.left)) self.x -= self.speed * dt;
        if (neko.input.key(.right)) self.x += self.speed * dt;
    }
    pub fn draw(self: *Player, at: neko.Point) void {
        neko.draw.rect(
            neko.Rect.init(@intFromFloat(self.x + @as(f32, @floatFromInt(at.x))), 160, 40, 40),
            neko.Color.hex(0x66ccff),
            true,
        );
    }
};

// The "scene": a function that builds a tree.
fn makeWorld(allocator: std.mem.Allocator, player: *Player) !*neko.Scene {
    const root = try neko.scene.create(allocator, .{ .name = "World" });
    _ = try neko.scene.addScript(root, player, .{ .name = "Player" });
    _ = try neko.scene.addLabel(root, .{ .text = "Hi", .pos = .{ .x = 320, .y = 30 } });
    return root;
}

var player = Player{};

pub fn main(init: std.process.Init) !void {
    var window = try neko.window.create(init, .{ .title = "Game", .width = 640, .height = 360 });
    defer window.close();

    window.switch_to(try makeWorld(neko.allocator(), &player));

    while (window.is_open()) {
        window.begin_frame();
        while (window.poll_event()) |ev| {
            switch (ev) {
                .quit => window.close(),
                .key_down => |key| if (key.key == .escape) window.close(),
                else => {},
            }
        }
        if (!window.is_open()) break;

        window.process(window.dt()); // ready on first frame, then process
        window.draw();               // clear + draw the tree
        window.present();
        window.end_frame();
    }
}
```

## Switching scenes

`window.switch_to(scene)` replaces the current scene and **frees it**, like
Godot's `change_scene_to`. To return to a scene you build it again:

```zig
if (std.mem.eql(u8, neko.scene.name(), "Menu")) {
    window.switch_to(try makePlay());
}
```

Use `window.push_scene` / `pop_scene` for overlays (pause, options) where the
scene underneath must stay alive.

## Finding nodes

```zig
const root = neko.scene.current().?;
const score = neko.scene.get_node("Hud/Score").?; // path from the scene root
const player = neko.scene.find("Player").?;       // depth-first by name
```

## Scripts without a backing struct

If you just want callbacks, use `neko.scene.Script` directly:

```zig
var ticks: u32 = 0;
const hooks = neko.scene.Script.Hooks{
    .process = struct {
        fn call(ctx: *anyopaque, dt: f32) void {
            _ = dt;
            @as(*u32, @ptrCast(@alignCast(ctx))).* += 1;
        }
    }.call,
};
_ = try neko.scene.Script.create(neko.allocator(), hooks, &ticks, .{ .name = "Ticker" });
```

Most of the time the struct + `addScript` form is nicer.
