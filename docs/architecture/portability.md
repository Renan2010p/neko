# Portability

Neko runs the same `src/core/**` on very different targets: hosted Linux/Windows
with SDL2, and the PlayStation 2 with a freestanding `mips64r5900el` target. This
page explains the rules that keep that possible, and how to add a platform.

## Portability is the boundary, not the target

"Freestanding" is a *target property* (`builtin.os.tag == .freestanding`), not
something a library is. Portability comes from keeping the core free of OS
assumptions and putting all OS work behind the backend interface.

The useful invariants:

1. **The core uses only allocator-parameterized `std`.** `std.mem.Allocator`,
   `ArrayListUnmanaged`, `StringHashMapUnmanaged`, `fmt`, `math`, `Random` —
   all of which work without an OS.
2. **The core never calls the OS.** No `std.fs`, no threads, no sockets. Files
   and devices go through `Backend`. `zig build check-freestanding` enforces
   this (it runs as part of `zig build test`).
3. **Platform differences are data, not code paths in the core.** Capabilities
   are queried at runtime (`neko.screen.supports_offscreen_targets()`), and the
   backend kind is a value (`neko.backend_kind`).
4. **Bootstrap lives in the backend.** A backend may need an allocator, an I/O
   context or an entry point; it receives them or provides them itself.

If you add something to `src/core/**` that needs an OS, put it on the
`Backend` vtable instead.

## Hosted vs freestanding

| Concern | Hosted (SDL2) | Freestanding (PS2) |
|---------|---------------|--------------------|
| allocator | `std.process.Init.gpa` | a libc `malloc`-backed allocator in the backend |
| entry | Zig's `start.zig` calls `main` | `entry.zig` exports `main` and builds `std.process.Init` |
| filesystem | `std.Io` inside the backend | none; the file ops return null/false |
| clock | `SDL_GetTicks64` | `GetTimerSystemTime` |
| libc | `link_libc = true` | links the PS2SDK libc from C |

The engine core is identical in both columns.

## What the core may rely on

- An `Allocator` supplied in `Config`.
- The `Backend` handle being valid after `neko.screen.init`.
- `neko.Backend` being the only interface to the outside world.

`Config.io` is **optional**: it exists only so a hosted backend can perform
file/asset access. The core stores just the allocator and `assets_dir`; it never
dereferences `io`. A freestanding backend leaves it `null` and its file
operations degrade to null/false.

## What a backend implements

The contract is **capability-based**: the vtable is folded from the modules under
`src/core/base/caps/`, and every field has a no-op default. A backend fills only
what it supports; the rest degrade gracefully (return null/false or do nothing),
exactly as the PS2 backend does.

- **`core`**: `init`, `shutdown`, `keeps_running`, `request_stop`, `present`, `ticks_ms`
- **`window`**: `set_logical_size`, `set_fullscreen`, `set_vsync`,
  `set_resolution`, `logical_size`, `display_modes`, `set_draw_offset`,
  `supports_curved_panorama`, `supports_offscreen_targets`
- **`graphics`**: `clear`, `draw_rect`, `draw_line`, `draw_circle`,
  `load_texture`, `create_target`, `draw_texture`, `draw_texture_rotated`,
  `texture_size`, `geometry`, `set_render_target`
- **`text`**: `load_font`, `draw_text`, `draw_text_rotated`, `text_size`
- **`audio`**: `load_sound`, `play_sound`, `stop_channel`, `stop_all_sounds`,
  `set_master_volume`, `set_sfx_volume`, `set_music_volume`
- **`files`**: `read_file`, `write_file`, `delete_file`, `file_exists`
- **`input`**: `poll_event`, `mouse_pos`
- **`misc`**: `update_discord`

The `3D` pipeline is optional and separate: a backend that has one sets
`Backend.render3d` and declares `.graphics3d`.

### Minimal backend skeleton

```zig
const engine = @import("neko");

pub const Engine = struct {
    // platform state...
    pub fn backend(self: *Engine) engine.Backend {
        return .{ .ptr = @ptrCast(self), .vtable = &vtable, .caps = caps_decl };
    }
};

var instance: Engine = .{};

/// Called by src/core/platform.zig.
pub fn create() engine.Backend {
    return instance.backend();
}

pub const kind: engine.BackendKind = .my_device;

const caps_decl: engine.Capabilities = blk: {
    var set: engine.Capabilities = engine.Capabilities.initEmpty();
    set.insert(.graphics2d);
    break :blk set;
};

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .draw_rect = vt_draw_rect,
    // ... anything omitted falls back to a no-op ...
};
```

The file lives at `src/backend/MyBackend/render/render.zig`. Then add a
`plugin: Backend` (a `build.zig` next to the renderer, importing
`src/backend/plugin.zig`) and register it in `src/backend/registry.zig` — never
`build.zig`. Add `.my_device` to `types.BackendKind`. Nothing in `src/core/**`
changes.

## Current status

| Backend | Target | Notes |
|---------|--------|-------|
| `.sdl2` | hosted | libc, `std.Io`, SDL2_ttf/image/mixer |
| `.sdl2_opengl` | hosted | SDL2 window + OpenGL presenter; the 3D pipeline |
| `.sdl2_vulkan` | hosted | SDL2 window + Vulkan presenter |
| `.ps2` | `mips64r5900el-freestanding` | gsKit, pad, audsrv; built with `-ofmt=c` |
| `.psx` | `mipsel-freestanding` | pure Zig, no SDK |
| `.headless` | any (including bare metal) | no OS, no window: the loop only |
| `.sdl3` | planned | not implemented (build panics with a message) |

The `headless` backend is the minimal **bare-metal reference**: it implements
only the `core` capability (a deterministic clock and a run flag) and needs no
libc, no window and no OS. Use it for tests, servers, or as the template for a
new device.

## Testing portability

`zig build test` runs the unit tests **and** `check-freestanding` (which fails if
`src/core/**` names an OS API). Both import only backend-free core modules.

`zig build check-targets` cross-compiles the core plus the `headless` backend
for a matrix of targets without linking or running:

```
x86_64-linux-gnu   aarch64-linux-gnu   riscv64-linux-gnu
x86_64-windows-gnu aarch64-macos-none  x86_64-macos-none
wasm32-wasi        wasm32-freestanding
arm-freestanding-eabi  thumb-freestanding-eabi
mips-freestanding  mipsel-freestanding  powerpc-freestanding
```

It covers 32/64-bit, big-endian (mips/powerpc) and wasm, so a core change that
assumes one architecture fails fast.

To keep the core honest, avoid importing `neko_backend` outside
`src/core/platform.zig`, and never call an OS API directly.
