# API reference

Everything is reached through `const neko = @import("neko");`. This page is a
map; the generated reference (`zig build docs`) has the full signatures and
doc comments.

## Types

| Type | Description |
|------|-------------|
| `neko.Color` | RGBA (0-255). `rgb`, `rgba`, `hex(0xRRGGBB)`, named colours, `withAlpha`, `fade`, `lerp` |
| `neko.Point` | integer `{ x, y }`, with `add`/`sub`/`scale`/`offset`/`eql` |
| `neko.Rect` | integer `{ x, y, w, h }`, with `init`, `fromPointSize`, `center`, `contains`, `overlaps`, `inset` |
| `neko.Vec2` | float 2D vector (also `neko.math.Vec2`) |
| `neko.TextureHandle` / `neko.SoundHandle` | opaque handles |
| `neko.Event` | tagged union of input/window events |
| `neko.Config` | startup config for `neko.screen.init` |
| `neko.BackendKind` | `.{ .name = "sdl2" }` (the core does not enumerate backends) |

## `neko.run` (and `neko.quit`)

The easiest entry point: hand it one frame callback.

```zig
pub fn main(init: std.process.Init) !void {
    try neko.run(init, .{
        .title = "Game", .width = 640, .height = 360,
        .clear_color = neko.Color.hex(0x0c0c12), // null = don't clear
        .max_dt = 0.05,
    }, update);
}

fn update(dt: f32) void { /* use neko.* */ }
```

- `neko.run(init, settings, frame) !void`
- `neko.quit()` — stop the loop after this frame
- `neko.running() bool`

`settings` is `neko.app.Settings` (`title`, `width`, `height`, `assets_dir`,
`fullscreen`, `vsync`, `load_default_font`, `clear_color`, `max_dt`).

## `neko.app`

The structured runner. `Game` is any struct with optional `start`, `event`,
`update(self, dt)`, `draw`, `stop` methods.

```zig
try neko.app.run(Game, &game, config);
try neko.app.runOptions(Game, &game, config, .{
    .load_default_font = true,
    .clear_color = neko.Color.hex(0x0c0c12), // null = don't clear
    .max_dt = 0.05,
});
try neko.app.frames(init, .{ .title = "Game" }, update); // same as neko.run
```

## `neko.window`

An explicit `Window` object for hand-written loops (`switch` over events). It
also owns the scene stack, so `switch_to` / `process` / `draw` replace the
Godot `change_scene_to` + `_process` + `_draw`.

```zig
var window = try neko.window.create(init, .{
    .title = "Game", .width = 640, .height = 360,
});
defer window.close();

window.switch_to(try makeMenu()); // frees the previous scene

while (window.is_open()) {
    window.begin_frame();               // drains events, updates input, computes dt
    while (window.poll_event()) |ev| { switch (ev) { ... } }
    if (!window.is_open()) break;

    window.process(window.dt()); // ready on the first frame, then process
    window.draw();               // clear + draw the current scene
    window.present();
    window.end_frame();
}
```

| Method | Description |
|--------|-------------|
| `create(init, options) !Window` | open the window (`error.WindowAlreadyOpen` / `error.InitFailed`) |
| `is_open() bool` | loop condition |
| `begin_frame()` / `end_frame()` | frame boundary; `begin_frame` updates input + `dt` |
| `dt() f32` | seconds since the previous frame |
| `poll_event() ?Event` | next event of the frame |
| `present()` | show the frame |
| `close()` | release everything (idempotent) |
| `switch_to(scene)` | make a scene current, freeing the previous one |
| `push_scene(scene)` / `pop_scene()` | overlay scenes (pause, options) |
| `process(dt)` / `draw()` | advance and draw the current scene |
| `current_scene() ?*Scene` / `scene_depth() usize` / `set_clear_color(color)` | scene state |
| `size() Point`, `set_logical_size`, `set_resolution`, `set_fullscreen`, `set_vsync`, `display_modes` | window state |

`options`: `title`, `width`, `height`, `assets_dir`, `fullscreen`, `vsync`.
Only one window per process; `neko.window.Options` is the settings type.

## `neko.splash`

The asset-free NEKO boot splash: the wordmark, a "loading" label and a progress
bar. `neko.run` and `neko.app.run` show it automatically before the first frame
(disable with `.splash = null`); with the explicit window API call
`window.splash(.{})`.

```zig
neko.splash.show(.{ .duration_ms = 1600 }); // blocking, any key skips

// Or drive it while loading real assets:
neko.splash.draw(loaded / total, .{}); // one frame; the caller presents
```

| Item | Description |
|------|-------------|
| `show(options)` | timed, blocking splash (skippable, handles quit) |
| `draw(progress, options)` | one frame for `progress` in `0..1` |
| `Options` | `duration_ms`, `skippable`, `bg`, `fg`, `accent`, `progress_bar`, `label` |

It draws with an embedded 5x7 bitmap font, so it works without any game asset
or `assets/font`.

## `neko.screen`

| Function | Description |
|----------|-------------|
| `init(config) bool` | open the window / prepare the backend |
| `shutdown()` | release everything |
| `present()` | show what was drawn this frame |
| `set_logical_size(w, h)` | virtual resolution scaled to the window |
| `set_fullscreen(on)` / `set_vsync(on)` / `set_resolution(w, h)` | window state |
| `logical_size() Point` | current virtual size |
| `display_modes(allocator) []DisplayMode` | list modes (caller frees) |
| `supports_curved_panorama()` / `supports_offscreen_targets()` | capability flags |
| `set_draw_offset(dx, dy)` | temporary translation of all draws |

## `neko.lifecycle`

- `keeps_running() bool` — true while the loop should continue.
- `request_stop()` — end after the current frame.

## `neko.input`

Two styles, usable together.

**State queries** (after `beginFrame`; `neko.run`/`neko.app` call it):

| Function | Meaning |
|----------|---------|
| `key(k) bool` | held down |
| `keyDown(k) bool` | pressed this frame |
| `keyUp(k) bool` | released this frame |
| `mouseDown(b) bool` | mouse button held |
| `mousePressed(b) bool` | mouse button pressed this frame |
| `mouseReleased(b) bool` | mouse button released this frame |
| `mouse() Point` | cursor position this frame |
| `wheel() f32` | wheel delta this frame |

**Raw events**:

- `poll_event() ?Event` — drain in a loop each frame.
- `mouse_pos() Point` — tracked position during a frame, else the backend.

**Frame lifecycle** (for hand-written loops):

- `beginFrame()` — drain the queue once, update state, auto-stop on `.quit`.
- `endFrame()`.

**Actions** (type-safe, no globals):

```zig
const Act = enum { jump, left, right };
var acts: neko.input.Actions(Act) = .{};
acts.bind(.jump, &.{ .{ .key = .space }, .{ .mouse = .left } });
acts.bind(.left, &.{ .{ .key = .left }, .{ .key = .a } });

if (acts.justPressed(.jump)) jump();
const move: f32 = acts.axis(.left, .right); // -1..1
```

`Actions(E)` has `bind(action, sources)`, `held`, `justPressed`,
`justReleased`, `axis(neg, pos)`. A `Source` is `.key(neko.Key)` or
`.mouse(neko.MouseButton)`; several sources can map to one action.

`Event` variants: `.quit`, `.key_down`, `.key_up`, `.mouse_motion`,
`.mouse_button_down`, `.mouse_button_up`, `.mouse_wheel`. A key event carries
`code: i32` (raw platform code), `key: neko.Key`, `name` and `scan_name`.
`neko.Key` covers arrows, Enter/Escape/Space/Tab/Backspace, `a`–`z` and `0`–`9`.

## `neko.draw`

- `clear(color)`
- `rect(rect, color, filled)`
- `line(x1, y1, x2, y2, color)`
- `circle(cx, cy, radius, color, filled)`
- `quad(p1, p2, p3, p4, color)` — filled convex quad
- `star(cx, cy, outer_r, color)`
- `text(str, x, y, options)` and `sprite(name, x, y, w, h)` — convenience aliases

`options` is `neko.text.Options`: `.size`, `.color`, `.center`, `.font`.

## `neko.texture`

- `load(path) ?TextureHandle`
- `create_target(w, h) ?TextureHandle`
- `create(w, h) ?TextureHandle` — CPU-writable streaming texture
- `fromPixels(w, h, pixels, pitch) ?TextureHandle` — create + initial upload
- `update(tex, pixels, pitch)` — re-upload pixels (packed ARGB; BGRA bytes on little-endian)
- `draw(tex, dst, src, alpha)`, `draw_rotated(tex, dst, angle, alpha)`
- `size(tex) Point`
- `geometry(tex, vertices, indices)` — textured mesh
- `set_target(target)` / `reset_target()` — offscreen rendering

## `neko.assets`

A name → loaded-resource registry with **visible errors** (unlike
`neko.sprite`, which silently falls back to a placeholder).

- `load_texture(name, path) !TextureHandle`
- `load_font(name, path, pixel_size) !i64`
- `load_sound(name, path) !SoundHandle`
- `texture(name) ?TextureHandle`, `font(name) ?i64`, `sound(name) ?SoundHandle`
- `unload(name)`, `unload_all()`, `deinit()` (called on `neko.screen.shutdown`)

Loads are idempotent and return `error.LoadFailed` when the backend refuses.
The registry owns the names; the backend owns the GPU/audio object.

## `neko.Camera2D`

A 2D camera that scrolls the world by shifting the core's draw calls, so it
works identically on **every** backend.

```zig
var cam = neko.Camera2D{ .position = .{ .x = 100, .y = 40 } };
cam.begin();   // world draws are now relative to the camera
// ... draw the world ...
cam.end();     // draw the HUD in screen space
```

Module `neko.camera2d`: `setPosition(p)`, `clear()`, `active() bool`,
`offset() Point`, `apply(p)`, `applyRect(r)`.

## `neko.text`

- `load_font(path, pixel_size) i64`
- `draw(str, x, y, options)` — returns nothing; silently skips if no font
- `draw_rotated(str, x, y, angle, options)`
- `size(str, font_idx) ?Point`
- `Options` — `.size = 24`, `.color = white`, `.center = false`, `.font = -1`

## `neko.sprite`

Named sprites (loaded from `<assets_dir>/<name>.png`), cached fonts and a few
widgets.

- `load(name)`, `get(name) ?TextureHandle`, `draw(name, x, y, w, h)`
- `face(name, x, y, w, h)` — draws only if present (no placeholder)
- `rounded(tex, x, y, w, h, radius, bg, alpha)`
- `text(str, x, y, options)`, `text_rotated(...)`
- `button(x, y, w, h, label, active, on_color, off_color)`
- `load_default_font()`
- `quality()`, `is_high_quality()`, `set_quality("high"/"low")`
- `clear_cache()`, `deinit()`

## `neko.effect`

Full-screen post effects: `noise`, `scanlines`, `vignette`, `tone`, `vhs`,
`camera`.

## `neko.render`

Higher-level presets. `cylinder(tex, .{ .source_width = ... })` wraps a flat
panorama around the viewer as a GPU mesh.

## `neko.shader`

Runtime shader programs, gated by the `shader` capability. When the active
backend cannot run shaders, `supported()` is `false` and every load returns
`null`, so callers can fall back to a CPU path. `neko.Shader` is an alias for
`neko.shader.Program`.

- `supported() bool`
- `load(vertex_src, fragment_src) ?Program` — GLSL source
- `load_files(allocator, dir, vertex_file, fragment_file) ?Program`
- `load_builtin(name) ?Program` — the engine's built-in `cylinder` shader
- `Program.draw(tex, params: [4]f32)` — draws over the whole screen, sampling
  `tex` and passing `params` as `u_params`
- `Program.deinit()`

```zig
const prog = neko.shader.load_files(neko.allocator(), "assets/shaders", "post.vert", "post.frag");
if (prog) |p| {
    defer p.deinit();
    p.draw(scene_tex, .{ 0, 0, 0, 0 });
}
```

## `neko.projection`

A **cylindrical panorama** projection with inverse mapping, so clicks can hit
hotspots placed inside the panorama. `neko.Cylinder` is an alias.

- `Cylinder.init(.{ .source_width = ... })` — options: `screen_width`,
  `screen_height`, `fov_deg`, `pan_max_deg`, `slice_width`, `mesh_step`
- `source_x(pan, x) f32` / `screen_x(pan, src) f32` — forward / inverse mapping
- `hit(pan, rect, x, y) bool` — is a screen point inside a source rectangle?
- `composite(tex, pan)` — per-column CPU composite
- `composite_mesh(tex, pan)` — GPU mesh (on backends with `geometry`)
- `composite_shader(tex, pan) bool` — runs the shader path; `false` when unsupported

## `neko.sound`

- `load(path) ?SoundHandle`
- `play(snd, loops, channel) i32` — `loops = -1` forever, `channel = -1` first free
- `stop_channel(channel)`, `stop_all()`
- `master_volume(v)`, `sfx_volume(v)`, `music_volume(v)` — 0..100

Object API: `sound.Sound.load(path) ?Sound`, `snd.play(loops, channel) i32`,
so a sound carries its own handle.

## `neko.time`

- `ticks_ms() u64` — monotonic milliseconds since startup.
- `dt() f32` — seconds since the previous frame (set by the runner).
- `elapsed() f32` — seconds since startup.

## `neko.random`

- `seed(value)`, `boolean()`, `probability(chance)`, `int_range(min, max)`,
  `float_range(min, max)`.

## `neko.math`

- `Vec2` with `add`, `sub`, `mul`, `scale`, `dot`, `length`, `distance`,
  `normalized`, `rotated`, `angle`, `clamp`, `lerp`, `toPoint`
- scalars: `clamp`, `clamp01`, `lerp`, `inverseLerp`, `smoothstep`,
  `moveTowards`, `damp`, `sign`, `wrap`, `toRadians`, `toDegrees`

## `neko.debug`

- `tick()` — call once per frame
- `fps() u32`, `draw_fps(x, y, color)`

## `neko.localization`

- `load(tables)`, `set_language(lang)`, `language()`, `text(key) []const u8`
  (falls back to `key`).

## `neko.save`

Pure serialization plus backend file helpers.

```zig
var w = neko.save.Writer.init(allocator);
defer w.deinit();
try w.int(score);
try w.str_u32("name");

var r = neko.save.Reader.init(w.slice());
const score = try r.int();
const name = try r.str_u32();
```

- `Writer`: `byte`, `flag`, `int` (i32), `uint` (u32), `size` (u64), `real`
  (f32), `str_u32`, `str_u64`, `raw`, `slice`
- `Reader`: the matching readers, plus `remaining()`
- `write_file(dir, name, data) bool`, `read_file(allocator, dir, name, max) ?[]u8`,
  `delete_file(dir, name)`, `file_exists(dir, name)` — all go through the backend

## `neko.scene`

A Godot-style node tree. A **scene** is a root node (alias `neko.Scene`).

- Manager: `init(allocator)`, `deinit()`, `create(allocator, options) !*Node`,
  `switch_to(root)`, `push(root)`, `pop()`, `current() ?*Node`, `depth()`,
  `name()`, `process(dt)`, `draw()`, `input(ev)`, `set_clear_color(color)`
- Lookup: `get_node("Hud/Score") ?*Node`, `find("Player") ?*Node`
- Builders: `addLabel`, `addSprite`, `addNode2D`, `addTimer`,
  `addAnimatedSprite`, `addAnimator`, `addScript` (and short aliases `label`,
  `sprite`, …)
- Node types: `Node`, `Node2D`, `Sprite`, `Label`, `Timer`, `AnimatedSprite`,
  `Animator`, `Script` (plus `Sprite2D`/`Label2D`/`AnimatedSprite2D` aliases)
- No-op hooks: `no_ready`, `no_process`, `no_draw`, `no_input`

### `neko.scene.Animator` (named clips)

`AnimatedSprite` plays one raw frame list; `Animator` registers **named clips**
and switches with `play("run")`:

```zig
const anim = try neko.scene.addAnimator(root, .{
    .clips = &.{
        .{ .name = "idle", .frames = &.{ "hero_0", "hero_1" }, .fps = 4 },
        .{ .name = "run",  .frames = &.{ "hero_2", "hero_3" }, .fps = 10 },
    },
    .play = "idle",
});
anim.play("run");                 // switch + restart
const frame = anim.currentFrame(); // the sprite name to draw
```

Also: `stop()`, `finished`, `Clip{ name, frames, fps, looping }`.

### `neko.scene.Script` (attach gameplay to a node)

A script is any struct with optional, public methods. Attach it with
`addScript`; the node forwards the tree lifecycle to it. The object must
outlive the node.

```zig
const Player = struct {
    x: f32 = 0,
    pub fn ready(self: *Player) void { _ = self; }
    pub fn process(self: *Player, dt: f32) void { self.x += 60 * dt; }
    pub fn draw(self: *Player, at: neko.Point) void { _ = self; _ = at; }
    pub fn input(self: *Player, ev: neko.Event) void { _ = self; _ = ev; }
};

var player = Player{};
_ = try neko.scene.addScript(root, &player, .{ .name = "Player" });
```

For callbacks without a backing type, build `neko.scene.Script.Hooks` and use
`neko.scene.Script.create(...)`.

### Node lookup

```zig
const score = neko.scene.get_node("Hud/Score"); // path from the scene root
const player = neko.scene.find("Player");        // depth-first by name
```

Paths support `/` to descend, `.` for the current node and `..` for the parent.
On any `*Node` you also have `node.getChild("Name")`, `node.find("Name")` and
`node.get_node("A/B")`.

See [godot.md](../guide/godot.md) for the full Godot → Neko mapping.

## `neko.net.discord_rich_presence`

Shows the game on Discord. It talks to the Discord desktop client over its
local IPC socket; when Discord is not running (or the backend has no support)
every call is a safe no-op.

- `connect(client_id) bool` — your Discord application id
- `set(presence) bool`
- `clear()`, `close()`, `connected() bool`

`presence` is `neko.DiscordPresence`: `details`, `state`,
`start_timestamp`/`end_timestamp` (Unix seconds), `large_image`/`large_text`,
`small_image`/`small_text`, `party_size`/`party_max`. Empty fields are omitted.

```zig
_ = neko.net.discord_rich_presence.connect("123456789012345678");
_ = neko.net.discord_rich_presence.set(.{
    .details = "Race · NARA COAST",
    .state = "Lap 1/3",
    .large_image = "rengear",
    .large_text = "RENGEAR",
});
```

Also available as `neko.discord`.

## `neko.platform`

Advanced. The backend seam (`backend_kind`, `Backend`, `create()`); most games
never need it.
