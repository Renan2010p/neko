# Porting Neko

Neko's goal is that bringing the engine to a new platform (a console, a new
OS, an embedded device) means writing backends, never touching the engine.
The engine only talks to the platform through four contracts, mirroring how
Godot splits its code between `platform/<name>` and `drivers/<api>`:

| Contract | What it covers | Godot equivalent | Crate |
|---|---|---|---|
| `neko_os::Os` + `FileSystem` | time, sleep, threads, entropy, logging, locale, files, save directory | `OS`, `FileAccess`, `DirAccess` | `neko-os` |
| `neko_platform::Platform` + `Window` | main loop, display, input, gamepads, suspend/resume | `DisplayServer`, `Input` | `neko-platform` |
| `neko_gpu::GpuBackend` | swap chain, buffers, textures, samplers, shaders, pipelines, command submission | `RenderingDeviceDriver` | `neko-gpu` |
| `neko_audio::AudioBackend` | open the output device and pull samples from the engine's mixer | `AudioDriver` | `neko-audio` |

## Backends that exist today

| Contract | Backends |
|---|---|
| OS | `StdOs` (every target with Rust's `std`) |
| Display | `neko-winit` (Wayland, X11, Win32, Cocoa, Android, Web), `neko-sdl3` |
| GPU | wgpu (Vulkan, Metal, D3D12, GL, WebGPU) and software — being moved onto `GpuBackend` |
| Audio | `DummyAudio`; SDL3 and cpal planned |

## Writing a port

Create a crate, for example `neko-myconsole`, that depends on the four
contract crates and implements what the platform needs. A typical console
port is:

1. **OS** — implement `Os` with the SDK's clock, threads, RNG and TTY, and a
   `FileSystem` over the game package (`res://`) and the save area (`user://`).
   If the target has a Rust `std`, `StdOs` may already be enough.
2. **Display** — implement `Platform::run` around the SDK's main loop and
   `Window` for the video output. Return `Surface::Platform(...)` from
   `Window::surface` with whatever your GPU backend needs (the display
   handle, the video output id); your GPU backend downcasts it.
3. **GPU** — implement `GpuBackend` on the platform's graphics API. Report
   `ShaderFormat::Native("<name>")` in `shader_formats` and ship shaders
   precompiled with the platform's compiler; the engine picks the matching
   code from each `ShaderDesc`.
4. **Audio** — implement `AudioBackend`: open the device and call
   `Mixer::mix` from its callback.
5. **Input** — translate pads and touch into `neko_platform::Event`s.

Start with `DummyAudio` and only the GPU features your game uses; the
`CommandList::validate` check and the shared backend tests help keep a new
backend honest.

## Licensing of ports

Neko is MPL-2.0. A port lives in its own crate and files, so it can keep the
license the platform holder requires (console SDKs are under NDA and cannot
be published). Changes to Neko's own files stay MPL-2.0 and open.
