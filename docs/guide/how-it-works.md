# How Neko works

> A guide focused on the control flow. For the contract details, see
> [../architecture/architecture.md](../architecture/architecture.md).

## The idea in one sentence

Neko is a **dispatcher**: you write `neko.something(...)`, the core translates it
into a call on an **abstract interface** (`Backend`), and the backend chosen at
build time (SDL2 on the desktop, gsKit on the PS2) does the real work.

You never talk to SDL. You talk to `neko.*`.

## The three pieces

```
   YOUR GAME               CORE (neko.*)              BACKEND
  (e.g. struct Game)      src/neko.zig + core/       src/backend/<Name>/
        │                        │                        │
        │  neko.draw.rect(...)   │                        │
        └───────────────────────►│  context.get()         │
                                 │  e.draw_rect(...)      │
                                 └───────────────────────►│  SDL_RenderFillRect
                                                          │  (or gsKit on the PS2)
```

- **Your game** only knows `neko.*`.
- **The core** knows no operating system; it only calls the interface.
- **The backend** does the heavy lifting (window, GPU, audio, keyboard, files).

## What happens at compile time

`build.zig` creates two modules:

- `neko` → `src/neko.zig` (the core).
- `neko_backend` → the selected backend, e.g.
  `src/backend/SDL2/render/sdl/render.zig` or
  `src/backend/PS2/render/render.zig`, according to `.backend`.

The core imports the backend underneath (in `src/core/platform.zig`), but your
game only imports `neko`:

```zig
const neko = @import("neko"); // resolves to src/neko.zig
```

That is why **switching backends does not change your game**: the file that
becomes `neko_backend` changes; `@import("neko")` stays the same.

## What happens at runtime (the `app.zig` example)

1. Zig calls `pub fn main(init: std.process.Init)`.
   - `init.gpa` is the allocator; `init.io` is the process I/O.
2. `var game = Game{}` — **your state**, created on the stack (x, y, vx, vy…).
3. `neko.app.run(Game, &game, config)` does:
   - **`screen.init(config)`**
     - stores `allocator` and `assets_dir` in the `context`;
     - `platform.create()` returns the backend (e.g. the `Sdl2Engine`);
     - `context.attach(handle)` registers that backend as the active one;
     - `handle.init(config)` opens the window, renderer and audio (via SDL2).
   - **`sprite.load_default_font()`** — loads the default font, if present.
   - **`if (@hasDecl(Game, "start")) game.start();`** → your `start` runs.
   - **Loop**, repeated while `neko.lifecycle.keeps_running()` is true:
     - `input.beginFrame()` — drains pending window events and updates key/mouse
       state.
     - `while (input.poll_event()) |ev| game.event(ev);` — your `event` gets
       `key_down`, `quit`, mouse, etc.
     - computes `dt` (seconds since the previous frame) from `time.ticks_ms()`,
       with a `max_dt` clamp.
     - `game.update(dt)` — your physics/movement.
     - `debug.tick()`, `draw.clear(color)`, `game.draw()` — your drawing.
     - `screen.present()` — shows the frame.
     - `input.endFrame()`.
   - When the window closes (`.quit`) or someone calls `request_stop()`, the loop
     ends. `defer screen.shutdown()` releases everything.

## How `neko.draw.rect` reaches the screen

```zig
neko.draw.rect(rect, color, true)
  → src/core/graphics/draw.zig        fn rect(...)
  → context.get()                     // ?Backend  (the backend registered at init)
  → e.draw_rect(rect, color, true)    // a method on the abstract interface
  → vtable.draw_rect(self.ptr, ...)   // jump into the backend
  → src/backend/SDL2/render/sdl/render.zig: SDL_SetRenderDrawColor + SDL_RenderFillRect
```

On the PS2, that same `rect()` ends in `gskit.prim.sprite`. Your game does not
know (or need to know) the difference.

## The `Game` struct callbacks

They are **all optional**: `neko.app` uses `@hasDecl(Game, "...")`, which decides
at compile time whether that method exists. If you do not declare `event`, it is
simply not called.

| Method | When it runs |
|--------|--------------|
| `start(self)` | once, after the window is ready |
| `event(self, ev)` | on every event (key, mouse, quit) |
| `update(self, dt)` | every frame; `dt` in seconds |
| `draw(self)` | every frame, after the `clear` |
| `stop(self)` | once, on exit |

`self: *Game` is the pointer to your instance. That is why `main` passes
`&game`: the runner needs the **type** (`Game`, for the introspection) and the
**pointer** (`&game`, to call the methods on your instance).

A Zig detail: if a method does not use `self`, you must discard it (`_ = self;`),
or the compiler complains about an unused parameter.

## Why `dt` matters

`update` runs once per frame, but frames have different durations. Multiply
everything that is a speed by `dt`:

```zig
self.x += self.vx * dt; // vx in pixels per SECOND
```

Without `dt`, the game is fast on fast machines and slow on slow ones.

## `event` vs `update`

- `event` is a **point in time**: the key was pressed *this* frame, the window
  was closed, the mouse moved. It has no `dt`.
- `update` is **continuous**: `dt` seconds passed, update the world.

Rule of thumb: instantaneous things (jump, menu, change scene) go in `event`;
continuous things (walk, gravity, animation) go in `update`.

## Three ways to run

- **Structured** (the `app.zig` example): you declare a struct with methods.

  ```zig
  try neko.app.run(Game, &game, config);
  ```

- **A single function** (less code, state-based input):

  ```zig
  try neko.run(init, .{ .title = "Game", .width = 640, .height = 360 }, update);

  fn update(dt: f32) void {
      if (neko.input.key(.right)) x += 160 * dt;
      neko.draw.rect(..., true);
  }
  ```

- **Explicit window + event `switch`** (you own the loop):

  ```zig
  var window = try neko.window.create(init, .{ .title = "Game", .width = 640, .height = 360 });
  defer window.close();

  while (window.is_open()) {
      window.begin_frame();                 // drains events and computes dt
      while (window.poll_event()) |ev| {
          switch (ev) {
              .quit => window.close(),
              .key_down => |key| if (key.key == .escape) window.close(),
              else => {},
          }
      }
      if (!window.is_open()) break;

      neko.draw.clear(neko.Color.hex(0x0c0c12));
      // ... draw ...
      window.present();
      window.end_frame();
  }
  ```

All three use the same core and the same backend. Pick whichever is most
comfortable: `neko.app` (methods), `neko.run` (one function), or `neko.window`
(explicit loop with a `switch`).

## Godot model (code-first)

Beyond the loop, the engine has Godot's runtime model, but **without an editor
and without `.tscn`**: a **scene** is a function/struct that builds a tree, and a
**script** is a `struct` with `ready`/`process`/`draw`/`input` methods.

```zig
const Player = struct {
    x: f32 = 300,
    pub fn process(self: *Player, dt: f32) void {
        if (neko.input.key(.right)) self.x += 180 * dt;
    }
    pub fn draw(self: *Player, at: neko.Point) void {
        neko.draw.rect(neko.Rect.init(@intFromFloat(self.x + @as(f32, @floatFromInt(at.x))), 160, 40, 40), neko.Color.hex(0x66ccff), true);
    }
};

var player = Player{};

fn makeWorld(alloc: std.mem.Allocator) !*neko.Scene {
    const root = try neko.scene.create(alloc, .{ .name = "World" });
    _ = try neko.scene.addScript(root, &player, .{ .name = "Player" });
    return root;
}

// ...
window.switch_to(try makeWorld(neko.allocator())); // swaps and frees the previous scene
```

- `window.switch_to(scene)` = Godot's `change_scene` (frees the previous one; to
  go back, build it again).
- `window.push_scene` / `pop_scene` = overlays (pause, options).
- `neko.scene.get_node("Hud/Score")` = `$Hud/Score`.
- `neko.scene.find("Player")` = search by name in the tree.

The full Godot → Neko mapping is in [godot.md](godot.md).
