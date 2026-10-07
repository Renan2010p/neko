# Neko

[![ci](https://github.com/Renan2010p/neko/actions/workflows/ci.yml/badge.svg)](https://github.com/Renan2010p/neko/actions/workflows/ci.yml)

A small, general-purpose 2D game engine for Zig, with pluggable backends.

Neko gives your game one platform-agnostic API — `neko.draw`, `neko.text`,
`neko.sprite`, `neko.effect`, `neko.sound`, `neko.input`, … — and hides the
platform behind a backend. The same engine core runs on the desktop (SDL2) and
on the PlayStation 2 (gsKit + PS2SDK, freestanding).

- Easy to start: a `Game` struct plus `neko.app.run` is a whole game.
- Modular: `src/core/**` never touches the OS; backends do the heavy lifting.
- Pluggable: a backend is a plugin registered in `build/backends.zig`; adding
  one never touches `build.zig` or the core.
- Documented: a guide in [`docs/`](docs/README.md).
- Portable: hosted and freestanding targets share the same core.

## Requirements

- Zig 0.16.0
- SDL2, SDL2_ttf, SDL2_image, SDL2_mixer (for the desktop backend)

```sh
sudo apt install libsdl2-dev libsdl2-ttf-dev libsdl2-image-dev libsdl2-mixer-dev
```

## A whole game

No game struct, no manual loop — one frame callback:

```zig
const std = @import("std");
const neko = @import("neko");

var x: f32 = 300;

pub fn main(init: std.process.Init) !void {
    try neko.run(init, .{
        .title = "My Game",
        .width = 640,
        .height = 360,
    }, update);
}

fn update(dt: f32) void {
    if (neko.input.key(.right)) x += 160 * dt;
    if (neko.input.keyDown(.escape)) neko.quit();

    neko.draw.rect(neko.Rect.init(@intFromFloat(x), 150, 40, 40), neko.Color.hex(0x66ccff), true);
    neko.text.draw("Hello, Neko!", 320, 60, .{ .size = 32, .center = true });
}
```

`neko.run` clears the screen, computes a clamped `dt`, calls `update` and
presents. Before the first frame it shows the asset-free **NEKO boot splash**
(disable with `.splash = null`). Input is queried directly (`neko.input.key`,
`keyDown`, `mouse`, `mousePressed`, …). Prefer methods? Use `neko.app.run` with
a game struct; want full control? Drive `neko.screen`/`neko.lifecycle` yourself.
See [docs/getting-started.md](docs/getting-started.md).

## Using it from a game

Add the dependency to your `build.zig.zon`:

```zig
.dependencies = .{
    .neko = .{ .path = "../neko" },
},
```

Pick a backend and get the module in `build.zig`:

```zig
const neko_dep = b.dependency("neko", .{
    .target = target,
    .optimize = optimize,
    .backend = .sdl2,
});
const neko = neko_dep.module("neko");
```

Switching `.backend` never changes your game code — the engine wires the
platform for you. See [docs/getting-started.md](docs/getting-started.md).

## pygame compatibility

A game written against pygame's model can be ported with `neko_pygame`, a
pygame-shaped software layer built only on Neko's public API:

```zig
const pg = @import("neko_pygame");

pub fn main(init: std.process.Init) !void {
    _ = pg.display.set_mode(512, 288);      // a virtual canvas surface
    try neko.app.run(Cfg, &game, .{ ... }); // draw in `frame`, then pg.display.flip()
}
```

| pygame                    | `neko_pygame`                                  |
|---------------------------|------------------------------------------------|
| `Surface`, `blit`, `fill` | `pg.Surface` (`pixels` are `0xAARRGGBB`)       |
| `Surface.set_alpha`       | `s.set_alpha(a)`                               |
| `pygame.draw.*`           | `pg.draw.rect/line/lines/circle/ellipse/polygon` |
| `pygame.transform.*`      | `pg.transform.scale/flip_x`                    |
| `pygame.Rect`             | `pg.Rect` (`colliderect`, `collidepoint`, `inflate`, …) |
| `display.set_mode/flip`   | `pg.display.set_mode` / `pg.display.flip`      |

Fonts, mixer buffers and rotation are not in the Zig shim; use `neko.text` /
`neko.sound` for those, or the Python bindings below.

## Running pygame games on Neko (Python)

There is a Python package that makes `import pygame` resolve to a Neko-backed
implementation, so existing pygame games run on this engine unchanged. The hot
paths (blit, draw, transform, flip) live in a CPython extension written in Zig
(`bindings/python/neko_ext.zig`); a thin pure-Python `pygame` package maps the
API (`Rect`, `Color`, events, constants).

```sh
zig build python                       # builds _neko.so and the pygame package
PYTHONPATH=$PWD/zig-out/python python3 your_game.py
# or run the bundled demo:
SDL_VIDEODRIVER=offscreen zig build python-demo
```

Requirements: the Python development headers (`python3-devel` on Void,
`python3-dev` on Debian) and, for `font`/`image`, Pillow. If your Python lives
elsewhere: `zig build python -Dpython-include=/path/to/python/include`.

Because `PYTHONPATH` is searched before `site-packages`, this shadows the real
pygame, so a game's `import pygame` is all it takes. See
[`bindings/python/README.md`](bindings/python/README.md) for the supported API
and roadmap.

## Backends are plugins

A backend lives in `src/platform/<name>/` and is registered in
[`build/backends.zig`](build/backends.zig). Adding one is three files — never
`build.zig`:

1. `src/platform/<name>/<name>.zig` exporting `kind` and `create() Backend`.
2. `build/backends/<name>.zig` with a `plugin: Backend` (metadata + how to
   build/link it).
3. A one-line registration in `build/backends.zig`.

| Name    | Status  | Notes |
|---------|---------|-------|
| `sdl2`  | working | SDL2 + SDL2_ttf/image/mixer |
| `ps2`   | working | gsKit + pad, freestanding, built with `-ofmt=c` |

Select one with `-Dbackend=<name>` (default `sdl2`), list them with
`zig build backends`, and read [docs/backends.md](docs/backends.md) to write
your own.

## Working on the engine

From this repository:

```sh
zig build            # build the modules
zig build test       # run the unit tests
zig build backends   # list the available backends
zig build docs       # write the API reference to zig-out/docs/api
zig build python     # build the pygame-compatible Python extension
zig build python-demo  # run the bundled pygame demo on Neko

zig build -Dbackend=ps2           # build against another backend
zig build test -Doptimize=Debug   # also: ReleaseSafe / ReleaseSmall
```

## Layout

```
src/neko.zig            public namespace root
src/platform.zig        the backend seam (the only core file that names a backend)
src/core/               types, dispatch namespaces, scene/state, math, serialization
src/compat/             compatibility layers (one directory per API)
src/compat/pygame/      the pygame-shaped layer (Surface/draw/transform/display)
src/platform/<name>/    one backend per platform (sdl2, ps2)
build/backends.zig      the backend registry
build/backends/<name>.zig  build plugin for one backend
bindings/python/        the CPython `_neko` extension + the `pygame` package
docs/                   the guide
```

The core depends only on the abstract `neko.Backend` interface; backends
implement it. See [docs/architecture.md](docs/architecture.md),
[docs/backends.md](docs/backends.md) and
[docs/portability.md](docs/portability.md).

## License

GNU General Public License v3.0 or later (GPL-3.0-or-later). See `LICENSE`.
See [CONTRIBUTING.md](CONTRIBUTING.md) to get involved and
[CHANGELOG.md](CHANGELOG.md) for what changed.

Copyright (C) 2026 Renan Lucas Vieira Hilário.
Created and directed by Renan Lucas Vieira Hilário, implemented with AI assistance.
