# Changelog

All notable changes to Neko are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
