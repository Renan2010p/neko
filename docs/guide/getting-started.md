# Getting started

## Requirements

- **Zig 0.16.0** (the version the engine is written against).
- For the desktop backend: SDL2 development packages for **SDL2**, **SDL2_ttf**,
  **SDL2_image** and **SDL2_mixer**.

On a Debian/Ubuntu system:

```sh
sudo apt install libsdl2-dev libsdl2-ttf-dev libsdl2-image-dev libsdl2-mixer-dev
```

On Fedora:

```sh
sudo dnf install SDL2-devel SDL2_ttf-devel SDL2_image-devel SDL2_mixer-devel
```

## Add Neko to your game

In your game's `build.zig.zon`:

```zig
.dependencies = .{
    .neko = .{ .path = "../neko" }, // or a URL + hash
},
```

In your game's `build.zig`, pick a backend and get the module:

```zig
const neko_dep = b.dependency("neko", .{
    .target = target,
    .optimize = optimize,
    .backend = .sdl2,
});
const exe = b.addExecutable(.{
    .name = "my_game",
    .root_module = b.createModule(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "neko", .module = neko_dep.module("neko") },
        },
    }),
});
b.installArtifact(exe);
```

Switching `.backend` never changes your game code.

## Your first program

The least boilerplate is a single frame callback via `neko.run`:

```zig
const std = @import("std");
const neko = @import("neko");

var x: f32 = 60;

pub fn main(init: std.process.Init) !void {
    try neko.run(init, .{
        .title = "My Game",
        .width = 640,
        .height = 360,
        .assets_dir = "assets",
    }, update);
}

fn update(dt: f32) void {
    if (neko.input.key(.right)) x += 120 * dt;
    if (neko.input.keyDown(.escape)) neko.quit();

    neko.draw.clear(neko.Color.hex(0x0c0c12)); // omit if you set clear_color
    neko.draw.rect(neko.Rect.init(@intFromFloat(x), 150, 40, 40), neko.Color.hex(0x66ccff), true);
    neko.text.draw("Hello!", 320, 60, .{ .size = 32, .center = true });
}
```

`neko.run` clears the screen (unless you pass `.clear_color = null`), computes a
clamped delta time, calls `update(dt)`, and presents. Input is queried with
`neko.input` (`key`, `keyDown`, `keyUp`, `mouse`, `mouseDown`, `mousePressed`,
`mouseReleased`, `wheel`). Closing the window stops the loop; `neko.quit()`
stops it yourself. Tuning lives in `neko.app.Settings`.

### Boot splash

`neko.run` and `neko.app.run` show the NEKO boot splash ("NEKO / loading" with a
progress bar) before the first frame. It is asset-free and skippable. Disable or
tune it with `Settings.splash`:

```zig
try neko.run(init, .{
    .title = "My Game",
    .splash = .{ .duration_ms = 2000 }, // or `.splash = null` to disable
}, update);
```

See [api.md](../reference/api.md#nekosplash).

### Prefer methods? Use a game struct

Declare a struct with any of `start`, `event`, `update(self, dt)` and `draw`,
then hand it to `neko.app.run`:

```zig
const Game = struct {
    x: f32 = 60,

    pub fn update(self: *Game, dt: f32) void {
        self.x += 120 * dt;
    }

    pub fn event(self: *Game, ev: neko.Event) void {
        _ = self;
        switch (ev) {
            .key_down => |key| if (key.key == .escape) neko.quit(),
            else => {},
        }
    }

    pub fn draw(self: *Game) void {
        neko.draw.rect(neko.Rect.init(@intFromFloat(self.x), 150, 40, 40), neko.Color.hex(0x66ccff), true);
    }
};

pub fn main(init: std.process.Init) !void {
    var game = Game{};
    try neko.app.run(Game, &game, .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "My Game",
        .width = 640,
        .height = 360,
    });
}
```

See [api.md](../reference/api.md#nekoapp) for the tuning options.

## The manual loop

If you want full control (fixed timestep, multiple phases, headless tests),
skip `neko.app` and drive the engine yourself:

```zig
pub fn main(init: std.process.Init) !void {
    const config: neko.Config = .{
        .allocator = init.gpa,
        .io = init.io,
        .title = "My Game",
        .width = 640,
        .height = 360,
    };
    if (!neko.screen.init(config)) return error.InitFailed;
    defer neko.screen.shutdown();

    while (neko.lifecycle.keeps_running()) {
        // Drain events once; beginFrame also fills neko.input's key/mouse state.
        neko.input.beginFrame();
        while (neko.input.poll_event()) |ev| {
            switch (ev) {
                .key_down => |key| if (key.key == .escape) neko.quit(),
                else => {},
            }
        }

        neko.draw.clear(neko.Color.hex(0x0c0c12));
        neko.text.draw("Hello!", 320, 180, .{ .size = 32, .center = true });
        neko.screen.present();
        neko.input.endFrame();
    }
}
```

Without `beginFrame`/`endFrame`, `neko.input.poll_event()` reads the platform
queue directly and the state queries stay empty; the two styles are
independent.

## Assets

Assets are per game and are resolved against `Config.assets_dir` (default
`"assets"`):

```
assets/
  player.png          -> neko.sprite.load("player")
  font/font.ttf       -> the default font for neko.text / neko.sprite.text
  blip.wav            -> neko.sound.load("assets/blip.wav")
```

Sprites are loaded by name (without extension) and cached. If a sprite is
missing, `neko.sprite.draw` renders a placeholder, so a game runs before its art
is ready. Text needs a font at `assets/font/font.ttf`.

## Next steps

- Read the [backends](../architecture/backends.md) guide to target another platform.
- Read the [architecture](../architecture/architecture.md) to understand the layering.
- Skim the [recipes](recipes.md) for common patterns.
