// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! neko — the public namespace root.
//!
//! This is the single import game code uses:
//!
//! ```zig
//! const neko = @import("neko");
//! ```
//!
//! ## The easy path
//!
//! Hand `neko.run` one frame callback. No struct, no manual loop:
//!
//! ```zig
//! var x: f32 = 300;
//!
//! pub fn main(init: std.process.Init) !void {
//!     try neko.run(init, .{ .title = "Hello", .width = 640, .height = 360 }, update);
//! }
//!
//! fn update(dt: f32) void {
//!     if (neko.input.key(.right)) x += 120 * dt;
//!     neko.draw.rect(neko.Rect.init(@intFromFloat(x), 160, 40, 40), neko.Color.hex(0x66ccff), true);
//! }
//! ```
//!
//! Input lives in `neko.input`: `key`, `keyDown`, `keyUp`, `mouse`,
//! `mouseDown`, `mousePressed`, `mouseReleased` and `wheel`. Closing the window
//! stops the loop; call `neko.quit()` to stop it yourself.
//!
//! ## The structured path
//!
//! Prefer methods over globals? Declare a game struct with optional `start`,
//! `event`, `update(self, dt)` and `draw`, then let `neko.app.run` drive it.
//!
//! ## The manual path
//!
//! Or drive the engine yourself (a custom fixed timestep, headless tests, …):
//!
//! ```zig
//! if (!neko.screen.init(config)) return error.InitFailed;
//! defer neko.screen.shutdown();
//! while (neko.lifecycle.keeps_running()) {
//!     neko.input.beginFrame();
//!     while (neko.input.poll_event()) |ev| { _ = ev; }
//!     neko.draw.clear(neko.Color.black);
//!     neko.screen.present();
//!     neko.input.endFrame();
//! }
//! ```
//!
//! ## Namespaces
//!
//! | Namespace            | Purpose                                             |
//! |----------------------|-----------------------------------------------------|
//! | `neko.Engine`        | an explicit engine handle (optional)                |
//! | `neko.app`           | run-loop helpers (`neko.app.run`, `neko.app.frames`)|
//! | `neko.run`           | run a game from a single frame callback              |
//! | `neko.quit`          | stop the main loop                                   |
//! | `neko.screen`        | startup, window state, presenting                   |
//! | `neko.window`        | an explicit `Window` object + `poll_event` loop     |
//! | `neko.splash`        | the asset-free NEKO boot splash                     |
//! | `neko.lifecycle`     | the run loop's stop condition                       |
//! | `neko.input`         | event queue and live key/mouse state                |
//! | `neko.draw`          | primitives (rect, line, circle, quad, star)         |
//! | `neko.texture`       | textures and offscreen render targets               |
//! | `neko.text`          | font loading and text drawing                       |
//! | `neko.sprite`        | named sprites, cached fonts, simple widgets         |
//! | `neko.effect`        | full-screen post effects (noise, scanlines, …)      |
//! | `neko.render`        | higher-level render presets (e.g. panorama cylinder)|
//! | `neko.sound`         | sound loading, playback and volume                  |
//! | `neko.math`          | allocation-free `Vec2`/scalar helpers               |
//! | `neko.time`          | monotonic clock                                     |
//! | `neko.random`        | seeded pseudo-random helpers                        |
//! | `neko.localization`  | a tiny key/text translation table                   |
//! | `neko.debug`         | frame diagnostics (FPS)                             |
//! | `neko.save`          | little-endian binary serialization + save files     |
//! | `neko.scene`         | a Godot-style node tree; a `Scene` is a root `Node`  |
//! | `neko.platform`      | the backend seam (advanced; backends and tools)     |
//!
//! `Backend` is the internal contract a platform implements; only backends
//! (`src/backend/...`) and the seam (`src/core/platform.zig`) reference it.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("core/base/types.zig");
const context: type = @import("core/base/context.zig");
const backend: type = @import("core/base/backend.zig");

// ── Version ──────────────────────────────────────────────────────────────

/// The engine version, mirroring `build.zig.zon`. Games show it in a menu with
/// `"Neko: v" ++ neko.version`.
pub const version: []const u8 = "0.1.0";

// ── Shared types ─────────────────────────────────────────────────────────

pub const Color: type = types.Color;
pub const Rect: type = types.Rect;
pub const Point: type = types.Point;
pub const DisplayMode: type = types.DisplayMode;
pub const RenderInfo: type = types.RenderInfo;
pub const TextureHandle: type = types.TextureHandle;
pub const SoundHandle: type = types.SoundHandle;
pub const ShaderHandle: type = types.ShaderHandle;
pub const Config: type = types.Config;
pub const Vertex: type = types.Vertex;
pub const BackendKind: type = types.BackendKind;

/// The backend this engine build was compiled with (chosen in the dependency
/// arguments, e.g. `.backend = .sdl2`).
pub const backend_kind: BackendKind = @import("core/platform.zig").kind;

// ── Events ───────────────────────────────────────────────────────────────

const event_mod: type = @import("core/system/event.zig");

pub const Event: type = event_mod.Event;
pub const Key: type = event_mod.Key;
pub const MouseButton: type = event_mod.MouseButton;

/// Convenience aliases for the event payloads (`Event.KeyEvent`, …).
pub const KeyEvent: type = event_mod.Event.KeyEvent;
pub const Motion: type = event_mod.Event.Motion;
pub const Button: type = event_mod.Event.Button;
pub const Wheel: type = event_mod.Event.Wheel;

// ── Backend contract (used by platform modules only) ─────────────────────

pub const Backend: type = backend.Backend;

/// A capability/feature a backend may declare (see `Backend.supports`).
pub const Feature: type = backend.Feature;

/// A set of declared backend capabilities.
pub const Capabilities: type = backend.Capabilities;

/// Registers the active backend so the namespaces below can dispatch.
pub fn attach(handle: backend.Backend) void {
    context.attach(handle);
}

/// Clears the active backend.
pub fn detach() void {
    context.detach();
}

/// The assets directory passed in `Config`.
pub fn assets_dir() []const u8 {
    return context.assets_dir;
}

// ── Explicit engine handle (optional) ────────────────────────────────────

/// An explicit engine handle. The global `neko.*` namespaces keep working; use
/// `Engine` when you want to start/stop the backend yourself (tools, tests,
/// embedding). See `neko.Engine`.
pub const Engine: type = @import("core/engine.zig").Engine;

/// The allocator passed in `Config`.
pub fn allocator() Allocator {
    return context.allocator;
}

// ── Platform seam (advanced) ─────────────────────────────────────────────

/// The bridge to the selected backend. Most games never touch this; it exists
/// for tools and backends. See `src/core/platform.zig`.
pub const platform: type = @import("core/platform.zig");

// ── High-level entry point ───────────────────────────────────────────────

/// Run-loop helpers. See `neko.app`.
pub const app: type = @import("core/app.zig");

/// Runs a game from a single frame callback — the least boilerplate:
///
/// ```zig
/// pub fn main(init: std.process.Init) !void {
///     try neko.run(init, .{ .title = "Game", .width = 640, .height = 360 }, update);
/// }
///
/// fn update(dt: f32) void {
///     neko.draw.clear(neko.Color.black);
/// }
/// ```
///
/// The runner clears the screen, computes a clamped `dt`, calls `frame`, then
/// presents. Input is available through `neko.input` (see its state queries),
/// and closing the window stops the loop automatically.
pub fn run(init: std.process.Init, settings: app.Settings, frame: *const fn (f32) void) !void {
    return app.frames(init, settings, frame);
}

/// Asks the main loop to end after the current frame (same as
/// `neko.lifecycle.request_stop`).
pub fn quit() void {
    lifecycle.request_stop();
}

/// True while the main loop should keep running (same as
/// `neko.lifecycle.keeps_running`).
pub fn running() bool {
    return lifecycle.keeps_running();
}

// ── Public namespaces ────────────────────────────────────────────────────

// System / platform services.
pub const lifecycle: type = @import("core/system/lifecycle.zig");
pub const time: type = @import("core/system/time.zig");
pub const random: type = @import("core/system/random.zig");
pub const localization: type = @import("core/system/localization.zig");
pub const input: type = @import("core/system/input.zig");
/// Type-safe input actions: `var acts: neko.input.Actions(MyEnum) = .{};`
pub const Actions: type = input.Actions;
pub const assets: type = @import("core/system/assets.zig");
pub const screen: type = @import("core/system/screen.zig");
pub const window: type = @import("core/system/window.zig");
pub const Window: type = window.Window;
pub const splash: type = @import("core/system/splash.zig");
pub const debug: type = @import("core/system/debug.zig");
pub const log: type = @import("core/system/log.zig");
pub const save: type = @import("core/system/save.zig");

// Integrations (talk to the outside world only through the backend).
pub const net: type = @import("core/net.zig");
pub const discord: type = net.discord_rich_presence;
pub const DiscordPresence: type = types.DiscordPresence;

// Graphics.
pub const draw: type = @import("core/graphics/draw.zig");
/// A 2D camera that scrolls the world on any backend.
pub const camera2d: type = @import("core/graphics/camera2d.zig");
pub const Camera2D: type = camera2d.Camera2D;
pub const texture: type = @import("core/graphics/texture.zig");
pub const text: type = @import("core/graphics/text.zig");
pub const sprite: type = @import("core/graphics/sprite.zig");
pub const effect: type = @import("core/graphics/effect.zig");
pub const render: type = @import("core/graphics/render.zig");
/// A cylindrical panorama projection with inverse mapping (clickable hotspots
/// inside the panorama). See `neko.projection`.
pub const projection: type = @import("core/graphics/projection.zig");
pub const Cylinder: type = projection.Cylinder;
/// Runtime shader programs (capability-gated). See `neko.shader`.
pub const shader: type = @import("core/graphics/shader.zig");
pub const Shader: type = shader.Program;
pub const render3d: type = @import("core/graphics/render3d.zig");
pub const Render3dVTable: type = @import("core/base/render3d.zig").VTable;

// Audio.
pub const sound: type = @import("core/audio/sound.zig");

// Math.
pub const math: type = @import("core/math.zig");
pub const Vec2: type = math.Vec2;
pub const Vec3: type = math.Vec3;
pub const Mat4: type = math.Mat4;

// 3D.
pub const camera: type = @import("core/3d/camera.zig");
pub const Camera: type = camera.Camera;
pub const mesh3d: type = @import("core/3d/mesh.zig");

// Scenes: a tree of `Node`s whose root is a `Scene` (Godot-style).
pub const scene: type = @import("core/scene/scene.zig");
pub const Scene: type = scene.Scene;
pub const Script: type = scene.Script;
pub const Animator: type = scene.Animator;
pub const Clip: type = scene.Clip;

// ── Tests ────────────────────────────────────────────────────────────────────
// Pulled in only by `zig build test` (a `test` block is ignored otherwise).
// The unit tests themselves live next to the code they test; this root also
// gathers any extra test-only files under `src/test/`.

test {
    _ = @import("test/root.zig");
}
