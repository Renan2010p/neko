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
│    backend.zig  abstract vtable│
│    context.zig  active backend │
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
| `src/backends/<name>/**` | the actual rendering, audio, input, files | yes |

### `src/core/backend.zig` — the contract

Everything a platform must supply is a single vtable: lifecycle, events, timing,
window, drawing primitives, textures, text, sound, files, input. The wrapper
methods (`Backend.draw_rect`, `Backend.write_file`, …) forward to `vtable`.

Game code never sees this type. Only backends implement it and only the core
calls it.

### `src/core/context.zig` — the active backend

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
screen lives. See [api.md](api.md) and [godot.md](godot.md).

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
| `.sdl2` | working | SDL2 + SDL2_ttf/image/mixer |
| `.sdl3` | planned | `zig build` panics with a clear message |
| `.ps2` | working | gsKit + pad, freestanding, built with `-ofmt=c` |

## Adding a backend

1. Create `src/backends/<name>/<name>.zig` with:
   - a concrete `Engine` struct,
   - `pub const Engine = ...;`, `pub const kind: neko.BackendKind = .<name>;`,
   - `pub fn create() neko.Backend` returning a stable instance,
   - the full `neko.Backend.VTable`.
2. Add the variant to `Backend` in `build.zig` and a `build_<name>` function
   that creates the `neko_backend` module (importing `neko`) and links whatever
   it needs.
3. Add the name to `types.BackendKind`.
4. Nothing in `src/core/**` changes.

See [portability.md](portability.md) for the rules a backend must respect.
