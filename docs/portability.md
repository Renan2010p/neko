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
   and devices go through `Backend`.
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

`Config.io` exists only so a hosted backend can perform file/asset access. The
core stores just the allocator and `assets_dir`; it never dereferences `io`.

## What a backend must implement

The full `neko.Backend.VTable`:

- **Lifecycle**: `init`, `shutdown`, `keeps_running`, `request_stop`, `present`
- **Events/timing**: `poll_event`, `ticks_ms`
- **Window**: `set_logical_size`, `set_fullscreen`, `set_vsync`,
  `set_resolution`, `logical_size`, `display_modes`,
  `supports_curved_panorama`, `supports_offscreen_targets`, `set_draw_offset`
- **Drawing**: `clear`, `draw_rect`, `draw_line`, `draw_circle`
- **Textures**: `load_texture`, `create_target`, `draw_texture`,
  `draw_texture_rotated`, `texture_size`, `geometry`, `set_render_target`
- **Text**: `load_font`, `draw_text`, `draw_text_rotated`, `text_size`
- **Sound**: `load_sound`, `play_sound`, `stop_channel`, `stop_all_sounds`,
  `set_master_volume`, `set_sfx_volume`, `set_music_volume`
- **Files**: `read_file`, `write_file`, `delete_file`, `file_exists`
- **Input/misc**: `mouse_pos`, `update_discord`

Unsupported features should degrade gracefully (return null/false or do
nothing) rather than fail, exactly as the PS2 backend does.

### Minimal backend skeleton

```zig
const engine = @import("neko");

pub const Engine = struct {
    // platform state...
    pub fn backend(self: *Engine) engine.Backend {
        return .{ .ptr = @ptrCast(self), .vtable = &vtable };
    }
};

var instance: Engine = .{};

/// Called by src/platform.zig.
pub fn create() engine.Backend {
    return instance.backend();
}

pub const kind: engine.BackendKind = .my_device;

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    // ... every field ...
};
```

Then wire it in `build.zig` (`build_my_device` → `neko_backend` module importing
`neko`) and add `.my_device` to `Backend` and `types.BackendKind`. Nothing in
`src/core/**` changes.

## Current status

| Backend | Target | Notes |
|---------|--------|-------|
| `.sdl2` | hosted | libc, `std.Io`, SDL2_ttf/image/mixer |
| `.ps2` | `mips64el-freestanding` | gsKit, pad, audsrv; built with `-ofmt=c` |
| `.sdl3` | planned | not implemented (build panics with a message) |

## Testing portability

The unit tests (`zig build test`) import only backend-free core modules, so
they run anywhere. The examples exercise the full core against the SDL2
backend. To keep the core honest, avoid importing `neko_backend` outside
`src/platform.zig`.
