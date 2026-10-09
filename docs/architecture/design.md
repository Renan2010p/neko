# Stable API design

> A proposal document. It defines what is **public and stable**, the
> **capability** model, and the path to **3D**, without breaking what exists.

## Goals

1. **OS-free core.** `src/core/**` only uses allocator-parameterized `std` — no
   `std.fs`, threads or sockets. `zig build check-freestanding` enforces this.
2. **Pluggable backends.** A backend is a self-contained folder; swapping it
   never changes the game.
3. **Stable API.** What a game uses (`neko.*`) grows additively; internal fields
   may change.
4. **3D from the start.** The backend contract and the public API leave room for
   an optional 3D pipeline, without forcing 2D backends to implement it.

## Three layers

```
game  ──uses──▶  neko.*  ──dispatches──▶  Backend (capabilities)  ──▶  backend
(never names the platform)   (core)          (contract)               (src/backend)
```

- **`neko.*`** — the game's API. Stable.
- **`Backend`** — the abstract contract (`src/core/base/`). Stable for backend
  authors.
- **`src/backend/<Name>/`** — implementations. Internal; loaded at build time.

## Public surface

| Namespace | Stability | Role |
|-----------|-----------|------|
| `neko.run`, `neko.app` | stable | run loop |
| `neko.Engine` | stable | explicit handle (optional) |
| `neko.screen`, `neko.window`, `neko.lifecycle` | stable | window and cycle |
| `neko.input` | stable | events and state |
| `neko.draw`, `neko.texture`, `neko.text` | stable | 2D |
| `neko.sprite`, `neko.effect`, `neko.render` | stable | high-level 2D |
| `neko.sound` | stable | audio |
| `neko.time`, `neko.random`, `neko.localization`, `neko.debug`, `neko.save` | stable | services |
| `neko.math` (`Vec2`, `Vec3`, `Mat4`, …) | stable | math |
| `neko.scene`, `neko.Script`, `neko.Node` | stable | 2D scene (Godot-like) |
| `neko.camera`, `neko.mesh3d`, `neko.render3d` | stable | 3D |
| `neko.Backend`, `neko.Feature`, `neko.Capabilities` | advanced | backend contract |
| `neko.platform` | internal | build seam |

### Versioning policy

- **SemVer on the package.** Adding a capability/namespace is *minor*; changing
  the signature of something stable is *major*.
- **The backend contract is additive.** New `VTable` fields get a *no-op*
  default (see capabilities), so an old backend keeps compiling.
- **`Feature` is the capability test.** Never test `backend_kind` to decide
  whether a feature exists; use `backend.supports(.feature)`.

## Capability model

The `VTable` is not hand-written: it is **folded at compile time** from
`src/core/base/caps/` (`core`, `window`, `graphics`, `text`, `audio`, `files`,
`input`, `misc`), and every field has a **no-op default**. A backend only names
what it implements.

```zig
// src/core/base/caps/core.zig
pub const VTable: type = struct {
    init: *const fn (ptr: *anyopaque, config: types.Config) bool = noopInit,
    keeps_running: *const fn (ptr: *anyopaque) bool = noopFalse,
    …
};
```

```zig
// src/backend/SDL2/render/sdl/render.zig
const caps_decl: engine.Capabilities = blk: {
    var set: engine.Capabilities = engine.Capabilities.initEmpty();
    set.insert(.graphics2d);
    set.insert(.text);
    set.insert(.audio);
    set.insert(.input);
    break :blk set;
};

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .draw_rect = vt_draw_rect,
    // everything else falls back to a no-op
};
```

`Feature` grows without breaking anything. Today:
`graphics2d`, `text`, `audio`, `files`, `input`, `graphics3d`,
`streaming_textures`, `offscreen_targets`, `geometry`, `display_modes`,
`curved_panorama`, `discord`.

## Standard backend layout

```
src/backend/
  plugin.zig           # (shared) the build-plugin contract
  registry.zig         # (shared) the backend registry
  <Name>/
    build_support.zig  # (optional) shared build helpers
    platform.zig       # the platform layer facade
    platform/          # SDKs, key maps, C headers, runtime
    entry.zig          # executable root (freestanding; optional when hosted)
    render/
      render.zig       # common render helpers (or the single renderer)
      <api>/render.zig # one presenter per graphics API (sdl, opengl, vulkan)
      <api>/build.zig  # that presenter's build wiring
```

The renderer module is published as `neko_backend`; its root is
`render/<api>/render.zig` (or `render/render.zig`). Because that root cannot
reach sibling files with `../`, the platform layer is published as a named
module (`neko_sdl2_platform`, `neko_ps2_platform`, …).

## `neko.Engine` — explicit handle (optional)

`neko.*` stays global. `Engine` (`src/core/engine.zig`) makes the active handle
explicit, useful for tools, tests and embedding:

```zig
var engine = neko.Engine.create(config);
if (!engine.init()) return error.InitFailed;
defer engine.shutdown();

while (engine.keeps_running()) {
    neko.input.beginFrame();
    // ...
    engine.present();
}
```

`create`, `init`, `shutdown`, `present`, `keeps_running`, `request_stop`,
`backend`, `allocator`, `assets_dir`, `supports`.

## 3D API

Today 3D is minimal: `render3d.draw(.cube, model, tint)` draws built-in meshes.
The proposal keeps that as a *fast path* and opens the rest additively.

### Meshes

```zig
pub const MeshHandle: type = struct { id: u32 };

// Built-ins (zero-cost; become a VAO/VBO per backend):
pub fn cube() MeshHandle;
pub fn quad() MeshHandle;
pub fn plane() MeshHandle;

// Built/loaded on the CPU:
pub fn create(vertices: []const mesh3d.Vertex, indices: []const u32) MeshHandle;
pub fn destroy(mesh: MeshHandle) void;
```

### Materials

```zig
pub const Material: type = struct {
    color: Color = .white,
    texture: ?TextureHandle = null,
    unlit: bool = false,
    // future: emissive, metallic, roughness
};
pub fn create(material: Material) MaterialHandle;
pub fn destroy(handle: MaterialHandle) void;
```

### Camera and submission

```zig
var cam = neko.Camera.init();          // already exists (position/rotation/fov/near/far)
cam.projection(aspect);                // already exists

neko.render3d.begin(cam);              // already exists (kept)
neko.render3d.draw(.cube, model, tint);        // current fast path (kept)
neko.render3d.drawMesh(mesh, model, material); // new
neko.render3d.end();                   // already exists
```

### 3D vtable extension

```zig
// src/core/base/render3d.zig (all with a no-op default)
pub const VTable: type = struct {
    begin3d: *const fn (ctx: *anyopaque, view: Mat4, fov: f32, near: f32, far: f32) void,
    draw3d: *const fn (ctx: *anyopaque, kind: mesh.Kind, model: Mat4, tint: types.Color) void,
    end3d: *const fn (ctx: *anyopaque) void,
    // additive:
    create_mesh: *const fn (ctx: *anyopaque, verts: []const mesh.Vertex, indices: []const u32) ?MeshHandle = noop,
    destroy_mesh: *const fn (ctx: *anyopaque, mesh: MeshHandle) void = noop,
    draw_mesh: *const fn (ctx: *anyopaque, mesh: MeshHandle, model: Mat4, material: Material) void = noop,
};
```

A backend without 3D leaves `Backend.render3d = null`; `neko.render3d` becomes a
no-op and `neko.render3d.supported()` answers `false`.

### 3D scene

The current `Node` is 2D. The proposal is an `Entity` (transform + parent/children)
and a `Scene3D`, reusing the `neko.scene` tree:

```zig
const e = try neko.scene3d.spawn(root, .{
    .mesh = neko.mesh3d.cube(),
    .material = .{ .color = neko.Color.hex(0xff8800) },
    .position = .{ .y = 1 },
});
neko.scene3d.rotate(e, .{ .y = 40 * dt });
```

`Entity` composes `position/rotation/scale`, `parent/children`, `mesh`,
`material` and a `visible` flag. The backend only receives
`draw_mesh(mesh, model, material)`; the hierarchy is resolved in the core.

### 3D phases (roadmap)

| Phase | Deliverable |
|-------|-------------|
| M1 ✅ | built-in meshes + OpenGL pipeline + `ursina` (spinning cube) |
| M2 | `MeshHandle`/`draw_mesh` + `Entity`/`Scene3D` (hierarchy, no texture) |
| M3 | `Material` with texture, unlit + one light; `Sky` |
| M4 | colliders + `raycast`/`boxcast`; `origin`, `look_at` |
| M5 | UI in `camera.ui` (`Text`, `Button`, `Panel`) |
| M6 | importers (`.obj`, glTF) and animation |

The Python/Ursina plan behind this is in [../guide/ursina.md](../guide/ursina.md).

## Conventions

- **Names**: `neko.<namespace>.<verb>` (`neko.draw.rect`, `neko.mesh3d.create`).
  Public types in PascalCase (`Camera`, `MeshHandle`).
- **Opaque handles**: structs with `id: u32` (`TextureHandle`, `SoundHandle`,
  `MeshHandle`) — the backend decides what they mean.
- **Graceful degradation**: a missing feature returns `null`/`false` or becomes a
  no-op, never a compile failure.
- **No hidden allocation**: functions that allocate take an explicit `allocator`
  (or the `Config.allocator`).
