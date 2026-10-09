# Architecture

Neko is built around one idea: **the engine core is platform-agnostic, and every
platform-specific thing lives behind a small abstract interface.** A game talks
only to the core; the core talks only to the interface; a backend implements the
interface.

```
        game / app code
              │  imports "neko"
              ▼
┌───────────────────────────────┐
│  src/neko.zig   public root   │
│  src/core/**    engine        │   no OS calls, allocator-driven
│    base/backend.zig  contract │
│    base/caps/*       per-cap. │
│    base/context.zig  active   │
│    engine.zig        handle   │
└───────────────┬───────────────┘
                │  src/core/platform.zig  (the only seam)
                ▼
        backend module "neko_backend"
                │
     ┌──────────┴──────────┐
     ▼                     ▼
  SDL2 (hosted)        gsKit (PS2, freestanding)
  libc + std.Io        PS2SDK + own allocator
```

## Layers

| Path | Responsibility | May touch the OS? |
|------|----------------|-------------------|
| `src/neko.zig` | public namespace root | no |
| `src/core/**` | types, dispatch namespaces, scene/state, math, serialization | **no** |
| `src/core/platform.zig` | the single seam that knows `neko_backend` | no (just wiring) |
| `src/backend/<Name>/**` | the actual rendering, audio, input, files | yes |

## The backend contract: capabilities

A platform supplies a **set of capabilities**, not one monolithic vtable. The
capability modules live in `src/core/base/caps/` — `core`, `window`, `graphics`,
`text`, `audio`, `files`, `input`, `misc` — and each declares its function
pointers **with a no-op default**:

```zig
// src/core/base/caps/core.zig
pub const VTable: type = struct {
    init: *const fn (ptr: *anyopaque, config: types.Config) bool = noopInit,
    keeps_running: *const fn (ptr: *anyopaque) bool = noopFalse,
    …
};
```

`src/core/base/backend.zig` **folds** those modules into one flat `Backend.VTable`
at compile time (`caps/merge.zig`), keeping every field's default. The payoff:

- A backend only names the capabilities it supports; the rest degrade to no-ops.
- The folded type is still a plain struct, so `vtable.draw_rect` keeps working
  and existing backends need no rewrite.

`Backend` also carries a `caps: Capabilities` set (`std.EnumSet(Feature)`).
Game code and tools query it with `backend.supports(.graphics3d)` instead of
testing for a concrete backend kind; new features are added to the `Feature`
enum without breaking existing backends.

The wrapper methods (`Backend.draw_rect`, `Backend.write_file`, …) keep the same
signatures and forward to `vtable`, so the dispatchers under `src/core/**` never
change. Game code never sees this type; only backends implement it and only the
core calls it.

### `neko.Engine` — an optional explicit handle

`neko.*` stays process-global. `neko.Engine` (`src/core/engine.zig`) makes the
active handle explicit (`create` / `init` / `shutdown` / `present` /
`keeps_running` / `supports`), which is useful for tools, tests and embedding.
See [design.md](design.md).

### `src/core/base/context.zig` — the active backend

When `neko.screen.init(config)` runs, the core asks `src/core/platform.zig` for a
backend handle and stores it in `context.current`. Every `neko.*` namespace
looks it up with `context.get()`:

```
neko.draw.rect(r, c, true)
    → draw.rect()
    → context.get()            // ?Backend
    → backend.draw_rect(...)   // vtable dispatch
    → platform implementation
```

`context` also remembers the allocator and the assets directory, both plain
data. It does **not** hold any OS handle.

### `src/core/platform.zig` — the seam

This is the only file in the core tree that names the concrete backend module
(`neko_backend`). It re-exports the backend kind and returns an abstract
`Backend` from `create()`. Adding a backend, or renaming one, touches this file
and `build.zig` — not `src/core/**`.

### Files go through the backend

The core never opens a file. `neko.save.write_file` / `read_file` /
`delete_file` / `file_exists` call the matching backend vtable entries. A
platform without a filesystem (the PS2 today) returns null/false, so saves
degrade to "no save file" instead of failing to compile. `Writer`/`Reader` are
pure and always available.

## Screens: `neko.scene`

Screens are a Godot-style tree of `Node`s (`Node2D`, `Sprite`, `Label`,
`Timer`, `AnimatedSprite`, `Script`). A **scene** is a root node; the manager
keeps a stack of them and `window.switch_to` swaps the current one (freeing the
previous), while `push_scene`/`pop_scene` handle overlays like pause menus.

There is exactly one screen system, so there is no ambiguity about where a
screen lives. See [api.md](../reference/api.md) and [godot.md](../guide/godot.md).

## The build

`build.zig` publishes two modules:

- **`neko`** — the engine (`src/neko.zig`).
- **`neko_backend`** — the backend selected by `.backend`.

The engine's `screen` module needs to instantiate the backend, so `neko`
imports `neko_backend`, and backends import `neko` for the engine types.
Cyclical imports are legal in Zig.

### Steps

From the engine repository:

| Command | Effect |
|---------|--------|
| `zig build` | build the modules (no artifacts) |
| `zig build test` | run the unit tests |
| `zig build backends` | list the available backends |
| `zig build docs` | emit the API reference to `zig-out/docs/api` |

### Backends

| Value | Status | Notes |
|-------|--------|-------|
| `.sdl2` | working | SDL2 + SDL2_ttf/image/mixer (`SDL_Renderer`) |
| `.sdl2_opengl` | working | SDL2 window + OpenGL presenter (+ 3D) |
| `.sdl2_vulkan` | working | SDL2 window + Vulkan presenter |
| `.ps2` | working | gsKit + pad, freestanding, built with `-ofmt=c` |
| `.psx` | working | pure-Zig, freestanding |
| `.sdl3` | planned | `zig build` panics with a clear message |

## The standard backend layout

```
src/backend/<Name>/
  platform.zig          the platform layer facade
  platform/             its files (SDK bindings, key maps, C headers, runtime)
  entry.zig             executable root (freestanding; optional when hosted)
  render/
    render.zig          common render helpers (or the single renderer)
    <api>/render.zig    one presenter per graphics API (sdl, opengl, vulkan)
```

A backend whose module root is deep (for example
`render/sdl/render.zig`) cannot reach sibling files with a relative import, so
the platform layer and the common render helpers are published as named modules
(`neko_sdl2_platform`, `neko_ps2_platform`, …) by the build plugin.

## Adding a backend

1. Create `src/backend/<Name>/render/render.zig` with:
   - a concrete `Engine` struct and a stable `instance`,
   - `pub const kind: neko.BackendKind = .<name>;`,
   - `pub fn create() neko.Backend` returning the instance handle,
   - the capability functions you support (the rest default to no-ops),
   - a `caps_decl: neko.Capabilities` listing your `neko.Feature`s.
2. Add a `build.zig` next to the renderer with a `plugin: Backend` (importing
   `src/backend/plugin.zig`) and publish the platform module(s) it needs.
3. Register it in `src/backend/registry.zig` and add the variant to
   `types.BackendKind`.
4. Nothing in `src/core/**` changes.

See [backends.md](backends.md) and [portability.md](portability.md).
