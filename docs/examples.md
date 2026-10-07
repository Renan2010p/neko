# Examples

Every example is a standalone program under `examples/`. They compile with
`zig build examples` and run with `zig build run-<name>` (the run steps need a
display).

| Name | Run | Shows |
|------|-----|-------|
| `hello` | `zig build run-hello` | least boilerplate: `neko.run` + a frame callback + input state |
| `app` | `zig build run-app` | the structured `neko.app.run` with `start`/`event`/`update`/`draw` |
| `window` | `zig build run-window` | an explicit `neko.window` + `poll_event` + `switch` |
| `scenes` | `zig build run-scenes` | a window with switchable scenes (`switch_to`) |
| `script` | `zig build run-script` | a plain object attached to the scene as a node |
| `splash` | `zig build run-splash` | the asset-free NEKO boot splash |
| `shapes` | `zig build run-shapes` | `neko.draw`: rect, line, circle, quad, star |
| `input` | `zig build run-input` | raw keyboard/mouse events, key names, clicks, wheel |
| `sprites` | `zig build run-sprites` | named sprites, placeholders, widgets, quality |
| `scene_tree` | `zig build run-scene_tree` | a `neko.scene` node tree with a repeating timer |
| `audio` | `zig build run-audio` | loading and playing a sound, volumes |
| `save` | `zig build run-save` | `Writer`/`Reader` and save files |

## Assets

The examples point `assets_dir` at `examples/assets`, which is intentionally
almost empty. They are written to run without any art or audio:

- missing sprites become grey placeholders,
- `neko.sound.load` returns `null` and playback is skipped,
- the save example creates its own directory.

Text is the one thing that needs an asset: drop a TTF at
`examples/assets/font/font.ttf` to see labels. Without it the shapes and
windows still run, but no glyphs are drawn.

To see the real asset paths in action, drop files in:

```
examples/assets/
  player.png
  logo.png
  font/font.ttf
  blip.wav
```

## Reading order

1. `hello.zig` — the least-boilerplate runner.
2. `app.zig` — the structured runner.
3. `shapes.zig`, `input.zig`, `sprites.zig` — the drawing and input APIs.
4. `window.zig`, `scenes.zig`, `script.zig` — the Godot-style scene model.
5. `audio.zig`, `save.zig` — audio and persistence.
