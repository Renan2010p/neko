# Ursina → Neko (3D)

Ursina is a code-first 3D game framework for Python. This document records how
Ursina works, what a Neko-compatible clone needs, and the phased plan to get
there. The 2D `neko_pygame` layer is the model: game code imports an Ursina-shaped
package, the heavy lifting lives in Zig, and the backend is chosen at build time.

## How Ursina works

Ursina is a thin, friendly layer on top of **Panda3D**. Almost everything is a
Panda3D node:

- `Entity` subclasses `NodePath`. An entity *is* a node in the scene graph, so
  `position`, `rotation`, `scale`, `parent` and `children` are the real
  Panda3D transform.
- `Ursina(ShowBase)` opens the window and owns the Panda3D task manager. The
  engine loop is a `taskMgr` task (`_update`) that advances `time.dt`, updates
  the mouse, calls the global `update()`, runs `Sequence`s, then calls
  `entity.update()` for every entity in `scene.entities`, and finally lets
  Panda3D render.
- Models are loaded from `.bam`/`.ursinamesh`/procedural classes
  (`Cube`, `Quad`, `Sphere`, `Cylinder`, `Cone`, `Plane`, `Terrain`, `Grid`, …).
  `model='cube'` is just a lookup by name.
- Materials are a `color` (Vec4) multiplied onto the vertex color plus an
  optional `texture`, driven by a GLSL `shader`. `Entity.default_shader` is
  `unlit_with_fog_shader`; `unlit_shader` ignores lights.
- Collision and picking are done with Panda3D colliders (`BoxCollider`,
  `SphereCollider`, `MeshCollider`, `CapsuleCollider`) and `raycast`/`boxcast`.
- Higher level widgets (`Text`, `Button`, `Panel`, `Slider`, `WindowPanel`,
  `TextField`, `Draggable`, …) are ordinary entities parented to `camera.ui`
  (2D overlay space).

### World conventions (important)

- Right-handed. **X right, Y up, Z forward** (`X × Y = Z`).
- The default camera is at `(0, 0, -20)` looking toward `+Z`, so objects spawn
  in front of it at `z ≈ 0`.
- `rotation` is in **degrees**, exposed as `rotation_x/y/z`, and internally
  mapped to Panda3D's HPR with `Entity.rotation_directions = (-1, -1, 1)`.
- `scale` is per-axis and never allowed to be exactly `0` (clamped to `.001`).
- `origin` shifts the model relative to its transform (e.g. `origin=(0, .5)`
  puts the pivot at the bottom of a cube).
- UI entities are parented to `camera.ui`; their `z` is used for 2D layering.

### The public surface (`from ursina import *`)

`Ursina`, `Entity`, `Vec2/Vec3/Vec4`, `color` (`Color`, `hsv`, `rgb`), `held_keys`,
`Keys`, `input_handler`, `camera`, `mouse`, `scene`, `window`, `time` (with
`time.dt`), `Text`, `Mesh`/`MeshModes`, `Shader`, `Texture`, `load_model`,
`load_texture`, `Sequence`/`Func`/`Wait`, `curve`, `destroy`, `duplicate`,
`raycast`, `boxcast`, `Audio`, `Sky`, the procedural models, and the prefabs
(`Button`, `Panel`, `Sprite`, `Slider`, `Draggable`, `TextField`, `WindowPanel`,
`Checkbox`, `ButtonGroup`, `ButtonList`, `Tooltip`, `Cursor`, `Animator`,
`Animation`, …), plus `every` and `SmoothFollow`.

### Hello world

```python
from ursina import *

app = Ursina()
ground = Entity(model='cube', color=color.magenta, scale=(50, 1, 10), y=-3,
                collider='box')
box = Entity(model='cube', color=color.orange, x=1, y=1)

def update():
    box.rotation_y += 40 * time.dt
    if held_keys['a']:
        box.x -= 4 * time.dt

app.run()
```

## What Neko needs

Ursina is a *3D* engine; Neko today is a 2D engine with a streaming-texture
presenter. Supporting Ursina-style code means adding a real 3D path:

| Layer | Neko today | Needed |
|-------|-----------|--------|
| Math | `Vec2`, scalar helpers | `Vec3`, `Vec4`, `Quat`, `Mat3`, `Mat4` |
| Geometry | `from_pixels` textures | `Mesh` (positions, normals, uvs, indices) + procedural models |
| Scene | 2D `Scene`/`Script` | `Entity` transform hierarchy (parent/children) |
| Render | 2D draw vtable + streaming texture | depth-tested 3D pipeline: camera, MVP, lighting, culling |
| Assets | `image` (PNG/…) | model importers (`.obj` first, then glTF/`.blend` off-line) |
| Collision | rects | AABB/OBB, sphere, ray/box cast |
| Python | `neko_pygame` | `src/compat/ursina` + a `_neko3d` extension |

## Proposed architecture

```
src/core/math/{vec3,vec4,quat,mat3,mat4}.zig   # pure, testable (Vec3/Mat4 done)
src/core/3d/
    mesh.zig        # Mesh: vertices, indices, bounds, procedural cube/quad/…
    entity.zig      # Entity: transform + parent/children + model/color/texture
    camera.zig      # Camera: fov, near/far, aspect, view + projection matrices
    scene.zig       # entity list, culling, render-order
    materials.zig   # color/texture/unlit, later a small lighting model
src/core/graphics/render3d.zig                 # submit meshes to the backend
```

The backend seam grows a small **3D block** in the vtable (all optional, so
`sdl2`, `ps2`, … keep working):

```zig
begin_3d:     *const fn (ptr, camera_view: Mat4, camera_proj: Mat4, clear_depth: bool) void,
draw_mesh:    *const fn (ptr, mesh: MeshHandle, model: Mat4, tint: Color, texture: ?TextureHandle, unlit: bool) void,
set_depth:    *const fn (ptr, enabled: bool) void,
end_3d:       *const fn (ptr) void,
```

- **OpenGL backend** implements it first: one depth-tested pipeline, a
  vertex/fragment shader with `MVP`, a single directional light, NEAREST
  sampling, back-face culling. This is the shortest path to a running 3D demo.
- **Vulkan backend** follows (same shader, `perspectiveVk`, depth attachment).
- **SDL_Renderer / PS2** leave the block `null`; `render3d` becomes a no-op or
  a CPU fallback.

On the Python side, mirror `neko_pygame`:

```
bindings/python/ursina/
    __init__.py     # re-exports; `from ursina import *`
    entity.py       # Entity (position/rotation/scale, parent/children, update/input)
    camera.py       # camera singleton
    application.py  # Ursina app, run(), window setup
    vec3.py, color.py, input_handler.py, ...
```

`Entity` is pure Python over a thin Zig core (mesh creation, matrix build,
`draw_mesh` submission), so the transform math stays in Zig and the object model
stays ergonomic in Python — same split as the pygame layer.

## Roadmap

| Phase | Deliverable | Status |
|-------|-------------|--------|
| **M0** | 3D math: `Vec3`, `Vec4`, `Quat`, `Mat3`, `Mat4`, camera matrices | Vec3 + Mat4 done; Vec4/Quat/Mat3 next |
| **M1** | Mesh (cube/quad/plane) + a depth-tested 3D pipeline in the OpenGL backend + `_neko3d` API; a spinning cube | next |
| **M2** | `Entity` transform hierarchy, `scene`, global `update`/`input`, `time.dt`, `held_keys` | — |
| **M3** | Colors, textures, unlit + one-light shaders, `Sky` | — |
| **M4** | Colliders + `raycast`/`boxcast`, `origin`, `look_at` | — |
| **M5** | UI: `Text`, `Button`, `Panel` (parented to `camera.ui`), basic prefabs | — |
| **M6** | Model importers (`.obj`, then glTF → `.ursinamesh`-like), `Animation`/`Animator` | — |

## API mapping (cheat sheet)

| Ursina | Neko (planned) |
|--------|----------------|
| `Ursina()` / `app.run()` | `ursina.Ursina()` on `neko.screen` + the 3D render loop |
| `Entity(model='cube', color=…, position=…)` | `ursina.Entity` over `neko.mesh` / `neko.render3d` |
| `Vec3` | `neko.math.Vec3` (done) |
| `camera.position/rotation/fov` | `neko.camera` |
| `time.dt` | `neko.time.dt` (already exists) |
| `held_keys['a']`, `input(key)` | `neko.input` (already exists; key names mapped) |
| `color.orange`, `hsv()` | `neko.color` helpers over `Color` |
| `raycast(...)` | `neko.physics.raycast` |
| `Text`, `Button`, `Panel` | 2D UI over `camera.ui` in `neko.render` + `neko.text` |
| `model='cube'` | `neko.mesh.cube()` |

## Notes and differences

- Ursina ships `.bam`/`.ursinamesh` assets built from Blender. Neko should ship
  procedural primitives and an offline converter so no Blender is required at
  runtime; `.obj` is the portable interchange format.
- Ursina's shader language is Panda3D GLSL (`p3d_*` uniforms). Neko uses its own
  shaders, so a `Shader` that is a *string path* must be re-authored; the common
  case (`unlit_shader`, `unlit_with_fog_shader`, `lit_with_shadows_shader`) maps
  to built-ins.
- Ursina is single-threaded and Python-heavy; keeping per-frame transform math
  and mesh submission in Zig is where Neko can win on framerate.
