# pygame on Neko (Python bindings)

This directory lets existing **pygame** games run on the Neko engine. Your
game keeps `import pygame`; the heavy paths execute in Zig.

```
your_game.py
   │  import pygame
   ▼
pygame/            pure-Python API (Rect, Color, events, constants)   ← this package
   │  ctypes-free direct C calls
   ▼
_neko.so           CPython extension written in Zig (the hot paths)
   │
   ▼
neko + neko_pygame  the engine (Surface blit/draw, window, present)
```

`_neko` deliberately defines **no Python types**: the Python layer owns the
`Surface` (a `bytearray` of `0xAARRGGBB` pixels) and passes it to Zig as a
buffer, so the pixel loops run natively while the API stays easy to extend.

## Build and run

```sh
zig build python                       # -> zig-out/python/{pygame,_neko.so,demo.py}
PYTHONPATH=$PWD/zig-out/python python3 your_game.py

# bundled demo (needs a display; use a virtual one headless)
zig build python-demo
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy NEKO_DEMO_FRAMES=120 zig build python-demo
```

Requirements:

- Python development headers (`python3-devel` on Void, `python3-dev` on Debian).
- SDL2 + SDL2_ttf/image/mixer (the `neko` desktop backend).
- Optional: Pillow, used by `pygame.font` and `pygame.image.load/save`.

If Python is not at `/usr/include/python3.14`:

```sh
zig build python -Dpython-include=/path/to/python/include
```

## Why `import pygame` works

`PYTHONPATH` is searched **before** `site-packages`, so a directory containing
this `pygame` package shadows the real one. Run your game with the install
directory on `PYTHONPATH` (or install it into a venv) and nothing else changes.

## Supported API (v0.1)

| Module | Status |
|---|---|
| `pygame` (constants, `Rect`, `Color`, `init`/`quit`) | ✅ |
| `pygame.display` (`set_mode`, `flip`, `update`, `get_surface`, `set_caption`, `quit`) | ✅ |
| `pygame.draw` (`rect`, `line`, `lines`, `circle`, `ellipse`, `polygon`, `aaline*`, `arc`) | ✅ |
| `pygame.transform` (`scale`, `smoothscale`, `flip`) | ✅ (`rotate`/`rotozoom` are copies for now) |
| `pygame.event` (`Event`, `get`, `poll`, `pump`, `post`, `clear`) | ✅ |
| `pygame.key` (`get_pressed`, `name`) | ✅ |
| `pygame.mouse` (`get_pos`, `get_pressed`) | ✅ |
| `pygame.time` (`get_ticks`, `delay`, `wait`, `Clock`) | ✅ |
| `pygame.font` (`Font`, `SysFont`, `render`, `size`) | ✅ via Pillow |
| `pygame.image` (`load`, `save`, `tobytes`/`tostring`, `frombytes`/`frombuffer`) | ✅ via Pillow |
| `pygame.locals`, `pygame.version` | ✅ |
| `pygame.mixer` | ⏳ stub (audio not wired yet) |
| `pygame.surfarray`, `pygame.gfxdraw`, `pygame.sndarray`, joysticks | ⏳ not yet |

## How the frame works

Neko's backend is driven manually instead of by `neko.app.run`:

1. `display.set_mode(size)` calls `_neko.init` (opens the window) and returns
   the screen `Surface`.
2. `event.get()`/`poll()`/`pump()` and `key.get_pressed()` call
   `_neko.frame_begin()` once per frame (drains SDL events into Neko's input
   state).
3. `display.flip()` uploads the surface as one streaming texture, presents, and
   calls `_neko.frame_end()`.

Pixels are packed `0xAARRGGBB` little-endian, matching Neko's streaming texture
format.

## Roadmap

- `mixer` wired to `neko.sound` (`Sound`, channels, volume).
- `transform.rotate`/`rotozoom` (SDL_RenderCopyEx is already in the backend).
- A real `pygame.surfarray` buffer protocol on `Surface`.
- Native `font` rendering through Neko (`neko.text`) instead of Pillow.
- Optional promotion to a fully native `pygame` extension if call overhead
  matters for a game.
