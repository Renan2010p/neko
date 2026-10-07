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
- Optional `-Dsdl2-include` / `-Dsdl2-lib` for building the extension without
  pkg-config (Windows).
- `docs/backends.md` (how to write a backend) and this changelog.
- CI: build, format check and unit tests on every push.

### Changed

- `zig build` defaults to **ReleaseFast**.
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
