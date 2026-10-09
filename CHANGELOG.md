# Changelog

All notable changes to Neko are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **2D ergonomics** (see [`docs/architecture/api-2d.md`](docs/architecture/api-2d.md)):
  - `neko.input.Actions(E)` — type-safe, per-instance input actions
    (`bind`, `held`, `justPressed`, `justReleased`, `axis`).
  - `neko.assets` — a name → resource registry with **visible errors**
    (`load_texture`/`load_font`/`load_sound`, getters, `unload`), freed on
    shutdown.
  - `neko.Camera2D` — a 2D camera that works on every backend by offsetting at
    the core (world draws shift; UI draws after `end()`).
  - `neko.scene.Animator` — named animation clips over sprite frames
    (`play("run")`), plus `neko.Clip`.
  - `neko.sound.Sound` — a loaded sound that carries its own handle.
- **`headless` backend**: a no-OS reference backend (a deterministic clock and a
  run flag only) for tests, servers and bare metal. Select it with
  `-Dbackend=headless`; it needs no libc and no window.
- **`zig build check-freestanding`**: fails if `src/core/**` references an OS API
  (`std.fs`, threads, POSIX, C bindings, raw `std.Io.Dir`/`File`); runs as part
  of `zig build test`.
- **Capability-based backend contract**: the vtable is now folded at compile
  time from `src/core/base/caps/**` (`core`, `window`, `graphics`, `text`,
  `audio`, `files`, `input`, `misc`), and every field has a no-op default. A
  backend only names the capabilities it supports. Backends also declare a
  `neko.Capabilities` set and answer `backend.supports(.feature)`.
- **`neko.Engine`**: an optional explicit engine handle
  (`create`/`init`/`shutdown`/`present`/`keeps_running`/`supports`), useful for
  tools, tests and embedding. The global `neko.*` namespaces are unchanged.
- **`docs/architecture/design.md`**: the stable-API and 3D design proposal.
- **3D rendering (M1)**: the `sdl2-opengl` backend now has a depth-tested 3D
  pipeline (MVP transform + one directional light, back-face culling off), with
  built-in `cube`, `quad` and `plane` meshes. Core: `neko.mesh3d`,
  `neko.Camera`, `neko.render3d` and an optional 3D block in the backend
  contract (`neko.Render3dVTable`); other backends leave it `null` and keep
  working. Python: a new `ursina` package (`Ursina`, `Entity`, `camera`,
  `color`, `held_keys`, `time.dt`, …) and `bindings/python/ursina_demo.py`
  (a spinning lit cube). Build it with `-Dbackend=sdl2-opengl`.
- **3D math foundation**: `neko.Vec3` and `neko.Mat4` (column-major, OpenGL
  layout) with translations, rotations, `lookAt`, GL/Vulkan perspective and
  transforms. `neko.math` now also re-exports `Vec3`/`Mat4`. This is the base
  for the 3D work described in [`docs/ursina.md`](docs/ursina.md).

- **Pluggable backend plugins**: backends are registered in
  `build/backends.zig`; adding one no longer touches `build.zig`. `zig build
  backends` lists them and `-Dbackend=<name>` selects one.
- **Python bindings** (`bindings/python`): a native CPython extension (`_neko`)
  plus a `pygame` package, so `import pygame` games run on Neko. The hot paths
  (a native `Surface` type, `draw`, blit, transform) are implemented in Zig.
- SDL2 rendering falls back to software when no GPU is available.
- **`sdl2-opengl` backend**: an SDL2 window with an OpenGL 3.3 core presenter
  (one streaming texture drawn as a full-screen quad). Select with
  `-Dbackend=sdl2-opengl`.
- **`sdl2-vulkan` backend**: an SDL2 window presenting through Vulkan
  (swapchain + graphics pipeline + streaming texture). Select with
  `-Dbackend=sdl2-vulkan`. SPIR-V shaders are precompiled under
  `src/backends/sdl2/shaders/`.
- `NEKO_VSYNC=0` disables vsync in the Python bindings (handy for benchmarks).

### Fixed

- `sdl2-opengl` and `sdl2-vulkan` now map SDL keys and mouse buttons onto
  `neko.Key`/`neko.MouseButton`, so `pygame.key.get_pressed()`, `keyDown()` and
  the mouse buttons work. Previously every key arrived as `unknown`, so games
  built with those backends ignored keyboard input. The mapping lives in
  `src/backends/sdl2/keys.zig` and is shared by every SDL2 backend.
- Optional `-Dsdl2-include` / `-Dsdl2-lib` for building the extension without
  pkg-config (Windows).
- `docs/backends.md` (how to write a backend) and this changelog.
- CI: build, format check and unit tests on every push.

### Changed

- **Backend kind decoupled from the core**: `neko.BackendKind` is now just a
  name declared by each backend (`.{ .name = "sdl2" }`) instead of a core enum,
  so adding a backend no longer edits `src/core/**`. A new
  `zig build check-targets` cross-compiles the core for 13 CPU/OS targets.
- **`Config.io` is optional** (`?std.Io`): a freestanding backend needs no
  process I/O, and file operations degrade to null/false when it is absent. The
  core stores only the allocator and the assets directory.
- **Unified backend layout**: every backend lives in `src/backend/<Name>/` with
  `platform.zig` + `platform/`, `entry.zig`, and `render/render.zig` (plus
  `render/<api>/render.zig` for multi-presenter platforms). The build plugin is
  co-located as `render/<api>/build.zig`; the registry is
  `src/backend/registry.zig` and the build contract `src/backend/plugin.zig`.
  There is no `build/backends/**` tree anymore.
- **Shared SDL platform layer**: `src/backend/SDL2/platform.zig` owns the SDL
  window, events, timing and files; `sdl2-opengl` and `sdl2-vulkan` reuse it and
  drop their no-op stubs.
- **Docs reorganized** into `docs/guide`, `docs/architecture` and
  `docs/reference`; `como-funciona.md` became `guide/how-it-works.md` (English).
- `zig build` defaults to **ReleaseFast**.
- **Source layout split into focused modules**: the pygame layer is now
  `src/compat/pygame/{pygame,pixel,rect,surface,draw,transform,display}.zig`
  (with `src/compat/compat.zig` as the compat root), and `neko.save` is split
  into `src/core/system/save/{writer,reader,files}.zig`. Public entry points
  (`neko_pygame`, `neko.save`) are unchanged.
- **Repository layout**: backends live in `src/backends/<name>/`, the seam is
  `src/core/platform.zig`, tests live under `src/test/`, and `src/neko.zig` is
  the only file at the root of `src/`.
- **`src/core/` modularized**: the foundation (`types`, `backend`, `context`)
  lives in `src/core/base/`, and `neko.math` is split into
  `src/core/math/{vec2,scalar}.zig` (with `math.zig` as the facade).
- `neko_pygame` draw primitives write pixels directly (no alpha blend on draw),
  matching pygame; blit is SIMD-accelerated.
- Various render fidelity fixes: solid ellipse rings, real `draw.arc`, thick
  lines, `border_radius`, float-precision polygon fill, pygame-compatible font
  metrics.

### Removed

- The `examples/` directory (the docs and `bindings/python/demo.py` cover the
  usage).

## [0.1.0]

Initial public snapshot: the engine core, the SDL2 and PS2 backends, the
`neko_pygame` software layer, the guide under `docs/`, and the unit tests.
