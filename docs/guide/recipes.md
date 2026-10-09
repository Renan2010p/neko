# Recipes

Short, copy-pasteable answers to common questions. All snippets assume:

```zig
const neko = @import("neko");
```

## Game loop with a fixed timestep

`neko.app` gives you a variable delta. For deterministic physics, accumulate:

```zig
const STEP = 1.0 / 60.0;

pub fn update(self: *Game, dt: f32) void {
    self.accumulator += dt;
    while (self.accumulator >= STEP) {
        self.fixedStep(STEP);
        self.accumulator -= STEP;
    }
}
```

`neko.app` already clamps spikes via `.max_dt`.

## Moving with `neko.math`

```zig
const m = neko.math;
// constant speed toward a direction
self.pos = self.pos.add(m.Vec2.init(1, 0).rotated(self.angle).scale(speed * dt));

// frame-rate-independent easing
self.opacity = m.damp(self.opacity, 1.0, 0.001, dt);

// approach a target without overshooting
self.zoom = m.moveTowards(self.zoom, 2.0, 3.0 * dt);
```

`Vec2` is float; convert with `.toPoint()` for drawing.

## Collision with rectangles

```zig
const a = neko.Rect.init(x, y, w, h);
const b = neko.Rect.init(px, py, pw, ph);
if (a.overlaps(b)) { /* hit */ }
if (a.contains(neko.input.mouse_pos())) { /* hovered */ }
```

## Reading input

Inside `neko.run` / `neko.app` the input state is ready to query:

```zig
if (neko.input.key(.left)) move(-1 * dt);      // held
if (neko.input.keyDown(.space)) jump();        // pressed this frame
if (neko.input.mousePressed(.left)) shoot();
const p = neko.input.mouse();
```

For raw events (text input, window events, wheel) poll them — they are the same
events the runner saw this frame:

```zig
while (neko.input.poll_event()) |ev| {
    switch (ev) {
        .mouse_wheel => |w| zoom += w.y * 0.1,
        else => {},
    }
}
```

Raw key codes remain available as `key.code` (e.g. `'w'`) for keys not in
`neko.Key`.

## Screen flow (Godot-style)

A screen is a scene. `window.switch_to` replaces the current scene and **frees
the previous one** (Godot's `change_scene`); to return, build it again.

```zig
window.switch_to(try makeMenu());

// on a menu action:
window.switch_to(try makePlay()); // frees the menu scene

// overlays keep the scene underneath alive:
window.push_scene(try makePause());
_ = window.pop_scene();
```

Use `push_scene`/`pop_scene` for pause/options overlays. See
[godot.md](godot.md).

## Scenes with `neko.scene`

```zig
neko.scene.init(allocator);
defer neko.scene.deinit();
neko.scene.set_clear_color(neko.Color.hex(0x101018));

const root = try neko.scene.create(allocator, .{ .name = "root" });
_ = try neko.scene.addLabel(root, .{ .text = "Hi", .pos = .{ .x = 320, .y = 40 } });
const hero = try neko.scene.addSprite(root, .{ .image = "player" });
_ = try neko.scene.addTimer(root, .{ .wait_time = 1.0, .one_shot = false });
neko.scene.switch_to(root);

// each frame:
neko.scene.process(dt);
neko.scene.draw();
```

## Saving a game

```zig
var w = neko.save.Writer.init(neko.allocator());
defer w.deinit();
try w.int(self.score);
try w.real(self.x);
try w.str_u32(self.player_name);
_ = neko.save.write_file("saves", "slot1.bin", w.slice());
```

Loading:

```zig
const bytes = neko.save.read_file(neko.allocator(), "saves", "slot1.bin", 1 << 20) orelse return;
defer neko.allocator().free(bytes);
var r = neko.save.Reader.init(bytes);
self.score = try r.int();
```

## Translations

```zig
const en = [_]neko.localization.Entry{
    .{ .key = "play", .value = "Play" },
};
const pt = [_]neko.localization.Entry{
    .{ .key = "play", .value = "Jogar" },
};
const tables = [_]neko.localization.Table{
    .{ .lang = "en", .entries = &en },
    .{ .lang = "pt", .entries = &pt },
};

neko.localization.load(&tables);
neko.localization.set_language("pt");
neko.text.draw(neko.localization.text("play"), 320, 180, .{ .center = true });
```

## Animated sprite

```zig
const idle = [_][]const u8{ "hero_idle_1", "hero_idle_2" };
const walk = [_][]const u8{ "hero_walk_1", "hero_walk_2", "hero_walk_3" };

const hero = try neko.scene.addAnimatedSprite(root, .{ .frames = &idle, .fps = 6 });
// later:
hero.play(&walk, 10, true);
```

## Debug FPS

```zig
neko.debug.tick();                       // once per frame (neko.app does it)
neko.debug.draw_fps(8, 8, neko.Color.yellow);
```
