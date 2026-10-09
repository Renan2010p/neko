# Writing a backend

Neko's core (`src/core/**`) talks to the OS only through the abstract
`neko.Backend` interface. A **backend** implements that interface and is wired
into the build as a plugin. Switching backends changes nothing in the core.

The contract is **capability-based**: the vtable is folded at compile time from
the modules under `src/core/base/caps/` (`core`, `window`, `graphics`, `text`,
`audio`, `files`, `input`, `misc`), and every field has a no-op default. A
backend only names the capabilities it supports; the rest degrade gracefully.

## The standard layout

Every backend uses the same layout, so SDL2, PS2 and PSX read the same way:

```
src/backend/<Name>/
  platform.zig          the platform layer facade
  platform/             its files (SDK bindings, key maps, C headers, runtime)
  entry.zig             executable root (freestanding; optional when hosted)
  render/
    render.zig          common render helpers (or the single renderer)
    <api>/render.zig    one presenter per graphics API
```

The renderer is a Zig module published as `neko_backend`; its root is
`render/<api>/render.zig` (or `render/render.zig` for a single-renderer
platform). Because that module root cannot reach sibling files with `../`, the
platform layer is published as its own named module
(`neko_sdl2_platform`, `neko_ps2_platform`, …) and imported by name.

## 1. `src/backend/<Name>/render/render.zig`

Implement the platform. A backend exports exactly three things and fills only
the capabilities it supports:

```zig
const engine: type = @import("neko");

pub const kind: engine.BackendKind = .{ .name = "<name>" }; // any name; the core does not enumerate them

pub fn create() engine.Backend { // one process-wide instance
    return instance.backend();
}
```

`create()` returns the handle the core stores. The concrete engine stays inside
your module. Build it by naming the capability functions you implement; anything
you omit keeps its no-op default:

```zig
const Engine: type = struct {
    // platform state...
    pub fn backend(self: *Engine) engine.Backend {
        return .{
            .ptr = @ptrCast(self),
            .vtable = &vtable,
            .caps = caps_decl, // see below
        };
    }
};

const caps_decl: engine.Capabilities = blk: {
    var set: engine.Capabilities = engine.Capabilities.initEmpty();
    set.insert(.graphics2d);
    set.insert(.input);
    break :blk set;
};

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .draw_rect = vt_draw_rect,
    // everything else falls back to a no-op
};
```

## 2. `src/backend/<Name>/platform.zig` (+ `platform/`)

The platform layer (SDK bindings, C headers, key maps, runtime) lives under
`platform/`, behind a `platform.zig` facade. If your renderer needs it, publish
it as a named module from the build plugin:

```zig
const platform = ctx.b.addModule("neko_mybackend_platform", .{
    .root_source_file = ctx.b.path("src/backend/MyBackend/platform.zig"),
    .target = ctx.target,
    .optimize = ctx.optimize,
});
```

## 3. Build wiring (`render/<api>/build.zig`)

Next to the renderer, describe how to build and link your module, importing the
shared contract at `src/backend/plugin.zig`:

```zig
const std = @import("std");
const backend = @import("../../../plugin.zig");

pub const plugin: backend.Backend = .{
    .name = "mybackend",
    .description = "one line shown by `zig build backends`",
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *std.Build.Module {
    const platform = ctx.b.addModule("neko_mybackend_platform", .{
        .root_source_file = ctx.b.path("src/backend/MyBackend/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
    });
    return ctx.b.addModule("neko_backend", .{
        .root_source_file = ctx.b.path("src/backend/MyBackend/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{
            .{ .name = "neko", .module = ctx.neko },
            .{ .name = "neko_mybackend_platform", .module = platform },
        },
    });
}

fn link(module: *std.Build.Module) void {
    // linkSystemLibrary(...) for the libraries your backend needs (no-op is fine).
    _ = module;
}
```

The renderer module **must** be published under the name `neko_backend` — that is
the stable import name the core's `src/core/platform.zig` seam looks up.

## 4. The registry (`src/backend/registry.zig`)

Add one entry to the registry, importing the build file you just wrote (here at
`src/backend/MyBackend/render/main/build.zig`):

```zig
const backend = @import("plugin.zig");
const sdl2 = @import("SDL2/render/sdl/build.zig");
const mybackend = @import("MyBackend/render/main/build.zig"); // <- add
const ps2 = @import("PS2/render/build.zig");

pub const all: []const backend.Backend = &.{
    sdl2.plugin,
    mybackend.plugin,
    ps2.plugin,
};
```

## Using it

```sh
zig build backends                # sdl2, mybackend, ps2
zig build test -Dbackend=mybackend
```

A game depending on Neko selects one with `.backend`:

```zig
const neko_dep = b.dependency("neko", .{ .target = target, .optimize = optimize, .backend = "mybackend" });
```

## Notes

- System-library directories (for platforms without pkg-config) are passed
  through `backend.Context`; the SDL2 plugin is the reference for using them.
- A backend that cannot work on a hosted target (like the PS2) sets
  `.hosted_only = false`; the developer steps are then skipped.
- `neko_pygame` and the Python bindings are pure consumers of the public API,
  so they pick up a new backend automatically.
- Declare a `caps` set so `neko.Backend.supports(.feature)` answers correctly.
