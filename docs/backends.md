# Writing a backend

Neko's core (`src/core/**`) talks to the OS only through the abstract
`neko.Backend` interface (a pointer + vtable, defined in
`src/core/backend.zig`). A **backend** implements that interface and is wired
into the build as a plugin. Switching backends changes nothing in the core.

## The three files

Adding a backend means creating three files — `build.zig` never changes:

### 1. `src/backends/<name>/<name>.zig`

Implement the platform, exporting exactly two symbols:

```zig
const engine = @import("neko");

pub const kind: engine.BackendKind = .<name>;   // add it to types.BackendKind

pub fn create() engine.Backend {                // one process-wide instance
    return instance.backend();
}
```

`create()` returns the handle the core stores; the concrete engine stays inside
your module. Implement every field of `engine.Backend.VTable` (see
`src/backends/sdl2/sdl2.zig` for a complete example).

### 2. `build/backends/<name>.zig`

Describe how to build and link your module:

```zig
const std = @import("std");
const backend = @import("../backend.zig");

pub const plugin: backend.Backend = .{
    .name = "mybackend",
    .description = "one line shown by `zig build backends`",
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *std.Build.Module {
    return ctx.b.addModule("neko_backend", .{
        .root_source_file = ctx.b.path("src/backends/mybackend/mybackend.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{.{ .name = "neko", .module = ctx.neko }},
    });
}

fn link(module: *std.Build.Module) void {
    // linkSystemLibrary(...) for the libraries your backend needs (no-op is fine).
    _ = module;
}
```

The module **must** be published under the name `neko_backend` — that is the
stable import name the core's `src/core/platform.zig` seam looks up.

### 3. `build/backends.zig`

Add one entry to the registry:

```zig
pub const all: []const backend.Backend = &.{
    sdl2.plugin,
    mybackend.plugin,   // <- here
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
