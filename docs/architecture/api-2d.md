# 2D API proposal

> A proposal for a **stable, easy-to-use 2D API**. It builds on what exists
> (`neko.draw`, `neko.sprite`, `neko.scene`, `neko.input`, …) and is additive:
> nothing here has to break today's code.

## Implemented today

- **Input actions** — `neko.input.Actions(E)` (`bind`, `held`, `justPressed`,
  `justReleased`, `axis`), type-safe and per-instance.
- **Asset registry with errors** — `neko.assets` (`load_texture`/`load_font`/
  `load_sound`, getters, `unload`), so missing files are caught at startup.
- **2D camera** — `neko.Camera2D`, backend-agnostic (it offsets at the core).
- **Named animation clips** — `neko.scene.Animator` / `neko.Clip`.
- **Sound object** — `neko.sound.Sound`.

Still proposed: atlas regions, node rotation/scale/z with `Vec2`, a single
`neko.App` entry, and `neko.ui`.

## Where Neko is today

**Good foundation**

- One platform-agnostic core; backends are pluggable capabilities
  ([design.md](design.md)).
- Three run styles: `neko.run` (callback), `neko.app.run` (methods),
  `neko.window` (manual loop).
- A Godot-like code-first scene tree: `Node`/`Node2D`/`Sprite`/`Label`/`Timer`/
  `AnimatedSprite`/`Script`.
- Immediate primitives (`neko.draw`, `neko.text`, `neko.texture`), sound, save,
  localization.
- Runs freestanding (`ps2`, `psx`, `headless`).

**What makes 2D games awkward**

| Symptom | Cause |
|---------|-------|
| "Which drawing way do I use?" | Immediate (`neko.draw`) and scene-tree (`neko.scene`) overlap; `scene.draw` even clears the screen itself. |
| Silent failures | A missing sprite/font/texture becomes a placeholder or a no-op; you cannot tell what went wrong. |
| Global singletons | `context`, the sprite/font caches, `input` state and the scene stack are process-wide; `Engine` is a thin veneer over them. |
| Integer-only transforms | `Point`/`Rect` are `i32`; nodes have position but no rotation/scale/z; no camera. |
| No asset registry | `sprite.load("hero")` builds `<assets>/hero.png` with a fixed buffer; no atlas, no errors, no unload. |
| Manual audio channels | `sound.play(handle, loops, channel)` pushes channel bookkeeping onto the game. |
| Raw input only | Keys/mouse exist, but no named actions, no rebinding, no gamepad. |
| Naming drift | `keyDown` vs `draw_rect`; `neko.sprite.draw` vs `neko.texture.draw`; comments still say `engine.*`. |
| `BackendKind` is core | Adding a backend edits `src/core/base/types.zig`, contradicting "adding a backend never touches the core". |

## Design principles

1. **One obvious path.** The scene tree is the recommended way to build a game;
   immediate `neko.draw` is the low-level escape hatch (HUDs, tools, debug).
2. **Fail loud by default, degrade only when asked.** Asset loads return errors
   (or log); no silent placeholders unless a debug flag is on.
3. **Floats for motion, ints for pixels.** `Vec2` for transforms/physics, `Point`
   (i32) only at the draw call boundary.
4. **Explicit is better, but short.** `neko.App.run(init, app, game)` should be a
   whole game; deeper control stays available.
5. **Additive and versioned.** New namespaces ship behind the same `neko.*`;
   existing calls keep working.

## Proposed surface

### 1. Input actions (over raw keys)

```zig
// Boot: bind once.
neko.input.bind(.jump, &.{ .key(.space), .button(.south) });
neko.input.bind(.left, &.{ .key(.left), .key(.a) });
neko.input.bind(.move_x, .axis2(.left, .right)); // -1..1

// Gameplay: read actions, not keys.
if (neko.input.justPressed(.jump)) player.jump();
const move: f32 = neko.input.axis(.move_x);
```

Raw `neko.input.key/mouse*` stays for menus and tools. Adds gamepad + rebinding
later through the same `bind` table.

### 2. Assets (registry with errors)

```zig
// Once, at startup.
try neko.assets.load_texture("hero", "art/hero.png");
try neko.assets.load_atlas("tiles", "art/tiles.png", .{ .tile = .{ .x = 16, .y = 16 } });
try neko.assets.load_font("ui", "font/ui.ttf", .{ .size = 16 });
try neko.assets.load_sound("hit", "sfx/hit.wav");

// Use.
const tex: neko.TextureHandle = neko.assets.texture("hero");
const font: neko.FontHandle   = neko.assets.font("ui");
```

- `load_*` returns `!Handle` (errors are visible).
- `unload(name)` / `unloadAll()` for lifetimes.
- An optional `.strict = false` mode restores the old placeholder behaviour.

### 3. 2D scene, transforms and camera

```zig
var world = neko.Scene.init(allocator, .{ .name = "world" });

const player = try world.add(neko.Sprite2D, .{
    .texture = neko.assets.texture("hero"),
    .position = .{ .x = 40, .y = 30 },
    .z = 1,
});

// Node2D gains rotation, scale and z; positions are Vec2 (f32).
player.node.position = player.node.position.add(.{ .x = 120 * dt });
player.node.rotation = angle;
player.node.scale = .{ .x = 2, .y = 2 };

// A 2D camera instead of manual offsets.
var cam = neko.Camera2D{ .position = player.node.position, .zoom = 2 };
neko.scene.set_camera(cam);
```

Keep `*neko.Scene` as the root `Node` (no breaking change); add the typed
`add`/`get` helpers and the transform fields.

### 4. Sprites and animation

```zig
try world.add(neko.Sprite2D, .{
    .texture = neko.assets.texture("tiles"),
    .region = neko.Rect.init(0, 16, 16, 16), // atlas sub-rect
    .flip = .x,
    .tint = .white,
});

const anim = try world.add(neko.Animator, .{
    .sprite = hero_sprite,
    .clips = &.{
        .{ .name = "idle", .frames = &.{0, 1}, .fps = 4, .loop = true },
        .{ .name = "run",  .frames = &.{2, 3, 4, 5}, .fps = 10, .loop = true },
    },
});
anim.play("run");
```

`Animator` generalizes today's `AnimatedSprite` (which plays a frame-name list)
to named clips with frames, fps, loop and events.

### 5. Audio objects

```zig
const Sound = neko.Sound; // loaded handle + play()
const hit: sound.Sound = try sound.load("sfx/hit.wav");
const voice: sound.Voice = hit.play(.{ .volume = 0.8, .pitch = 1.1 });

const music: sound.Music = try sound.stream("music/level.ogg");
music.play(.{ .loop = true, .fade = 0.5 });
```

`Sound`/`Music`/`Voice` replace manual channel juggling; `neko.sound` keeps the
low-level `play/stop_channel/volume` functions.

### 6. UI

```zig
const ui = neko.ui.layer(); // anchored to the logical screen, not the camera
const score = try ui.add(neko.ui.Label, .{ .text = "0", .anchor = .top_left, .pos = .{ .x = 8, .y = 8 } });
try ui.add(neko.ui.Button, .{ .text = "Play", .anchor = .center, .on_press = start });
score.set_text("100");
```

A small anchored layout (`anchor`, `margin`, `size`) on top of the existing
`text`/`rect` primitives.

### 7. The app contract

One recommended entry; the other two stay as lower layers:

```zig
const Game = struct {
    player: Player = .{},

    pub fn start(self: *Game) void { neko.input.bind(...); }
    pub fn update(self: *Game, dt: f32) void { self.player.update(dt); }
    pub fn draw(self: *Game) void { /* world + ui draw themselves */ }
};

pub fn main(init: std.process.Init) !void {
    try neko.App.run(init, .{
        .title = "My Game",
        .size = .{ .x = 320, .y = 180 },
        .clear = neko.Color.hex(0x101018),
        .assets = "assets",
    }, &game);
}
```

`neko.App` = today's `neko.app.run` + asset loading + a single owned
`Scene` + fixed `Options`, so the frame order (input → update → draw → present)
is unambiguous.

## Stability and migration

- **New namespaces are additive**: `neko.assets`, `neko.ui`, `neko.App`,
  `neko.Camera2D`, `neko.input.bind`. Today's `neko.draw`, `neko.sprite`,
  `neko.text`, `neko.sound`, `neko.scene` keep their signatures.
- The **scene tree becomes the recommended path**; `neko.draw` stays supported
  for immediate use.
- `AnimatedSprite` gets a doc-note pointing at `Animator`; no removal yet.
- `Point` stays for draw calls; nodes move to `Vec2` in a minor release (both
  accepted at first).
- Add a `@hasDecl`-free, single `BackendKind` extension: move the enum out of
  core (see [design.md](design.md)) so adding a backend stops touching
  `src/core`.

## Priority order

| Priority | Item | Why |
|----------|------|-----|
| P0 | Assets registry with errors | Every game hits missing assets; silence is the #1 DX killer. |
| P0 | Input actions | Games think in actions, not keycodes. |
| P1 | Node2D transforms + `Camera2D` | Required for anything that moves smoothly or scrolls. |
| P1 | `Animator` + atlas regions | The core of 2D characters and tiles. |
| P2 | `neko.App` single entry | Removes "which loop?" confusion. |
| P2 | `Sound`/`Music` objects | Removes channel bookkeeping. |
| P3 | `neko.ui` layout | Menus/HUD without hand-rolled rect math. |

Everything above is 2D-first and leaves the 3D plan in
[design.md](design.md) untouched.
