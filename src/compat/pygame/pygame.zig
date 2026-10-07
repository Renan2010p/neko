// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko_pygame` — a small, pygame-shaped compatibility layer on top of Neko.
//!
//! It gives a game the pygame model: a CPU `Surface` you draw into with
//! primitives, alpha blits, `transform` scale/flip, and a `display` that
//! presents one canvas to the window. It only uses Neko's *public* API
//! (textures + screen), so it never touches the engine core or the backends.
//!
//! ```zig
//! const pg = @import("neko_pygame");
//!
//! pub fn main(init: std.process.Init) !void {
//!     const screen = pg.set_mode(512, 288);
//!     try neko.run(init, .{ .width = 1024, .height = 576, .clear_color = null }, frame);
//! }
//!
//! fn frame(dt: f32) void {
//!     const s = pg.surface();
//!     s.fill(pg.Color.hex(0x101018));
//!     pg.draw.circle(s, 120, 90, 40, pg.Color.hex(0x4fd97a), true);
//!     pg.flip();
//! }
//! ```
//!
//! Pixels are packed `0xAARRGGBB` (matching Neko's streaming texture format).
//!
//! This file is only the public facade; the implementation is one module per
//! concern (all in this directory):
//!
//! | Module            | Contents                                  |
//! |-------------------|-------------------------------------------|
//! | `pixel.zig`       | packing + the (SIMD) alpha blend          |
//! | `rect.zig`        | `Rect`                                    |
//! | `surface.zig`     | `Surface`                                 |
//! | `draw.zig`        | `draw.*` primitives                       |
//! | `transform.zig`   | `transform.scale/flip_x`                  |
//! | `display.zig`     | the canvas + `set_mode`/`flip`            |

const neko: type = @import("neko");
const pixel: type = @import("pixel.zig");
const rect_mod: type = @import("rect.zig");
const surface_mod: type = @import("surface.zig");
const draw_mod: type = @import("draw.zig");
const transform_mod: type = @import("transform.zig");
const display_mod: type = @import("display.zig");

// ── Types ────────────────────────────────────────────────────────────────────

/// Alias of `neko.Color` (RGBA). Use `Color.hex(0xRRGGBB)` like pygame.
pub const Color: type = neko.Color;
/// Alias of `neko.Point` (integer pixel position).
pub const Point: type = neko.Point;
/// A floating-point vertex (for sub-pixel polygon fills).
pub const PointF: type = pixel.PointF;
pub const Rect: type = rect_mod.Rect;
pub const Surface: type = surface_mod.Surface;

// ── Namespaces ───────────────────────────────────────────────────────────────

pub const draw: type = draw_mod.draw;
pub const transform: type = transform_mod.transform;
pub const display: type = display_mod.display;

// ── display helpers (pygame-style top level) ─────────────────────────────────

pub const init: *const fn () void = display_mod.init;
pub const quit: *const fn () void = display_mod.quit;
pub const set_mode: *const fn (u32, u32) *Surface = display_mod.set_mode;
pub const surface: *const fn () *Surface = display_mod.surface;
pub const flip: *const fn () void = display_mod.flip;

// ── Pixel helpers (advanced) ─────────────────────────────────────────────────

pub const pack: *const fn (Color) u32 = pixel.pack;
pub const unpack: *const fn (u32) Color = pixel.unpack;
