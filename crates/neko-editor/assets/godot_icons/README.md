# Godot editor icons

The SVGs in this directory are node class icons copied from the
[Godot Engine](https://github.com/godotengine/godot) source tree, so Neko's
scene tree, inspector and Create Node dialog use the same artwork as Godot.

- Source: `editor/icons/*.svg` at tag **4.7.2-stable**
- Commit: `ed1daf0bf001b61586d9930840f2f1394092c079` (see `UPSTREAM`)
- License: MIT, the same as Godot — see `LICENSE.txt` and `COPYRIGHT.txt`

Only the icons for classes in `neko-core`'s class database and the editor
actions Neko uses are vendored. `build.rs` rasterizes them into one atlas at
build time (56 px per icon plus a transparent margin), so no SVG parser ships
in the editor binary.

Class icons keep Godot's category colors (2D blue, 3D red, UI green, core
gray); action icons are light gray and are tinted by the caller. A class
without its own file inherits the icon of its nearest ancestor, matching
Godot's lookup. `Light3D` is intentionally absent: that tag has no file for
it, so it falls back to `Node3D.svg`.

## Updating

1. Copy new or changed `.svg` files from `editor/icons/` at the desired Godot
   tag, keeping the original file names.
2. Update the tag and commit in this file and `UPSTREAM`.
3. `cargo build -p neko-editor` regenerates the atlas automatically.

Some class names differ between Neko and Godot (`GpuParticles2D` vs
`GPUParticles2D`, `HttpRequest` vs `HTTPRequest`); those aliases live in
`src/node_icons.rs`, and the action icon mapping lives in `src/icons.rs`.
