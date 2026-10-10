// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! **Common render helpers** shared by the SDL2 presenters.
//!
//! Each presenter lives in its own folder (`sdl/`, `opengl/`, `vulkan/`) and
//! implements the `neko.Backend` vtable on top of the shared SDL layer
//! (`../platform.zig`). The maths that every presenter repeats — turning a
//! logical-pixel rectangle into normalised device coordinates — lives here
//! once.

const engine: type = @import("neko");

/// A textured-quad vertex as uploaded by the streaming presenters.
pub const QuadVertex: type = extern struct {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
};

/// A rectangle in normalised device coordinates (`-1..1`), Y up.
pub const NdcRect: type = struct {
    x0: f32,
    x1: f32,
    /// The top edge (`rect.y`), in NDC.
    y_top: f32,
    /// The bottom edge (`rect.y + rect.h`), in NDC.
    y_bottom: f32,
};

/// Converts a logical-pixel rectangle into NDC for a `logical_w` × `logical_h`
/// viewport. The result is Y-up; the Vulkan presenter flips it when packing its
/// push constants.
pub fn ndcRect(rect: engine.Rect, logical_w: u32, logical_h: u32) NdcRect {
    const w: f32 = if (logical_w > 0) @floatFromInt(logical_w) else 1;
    const h: f32 = if (logical_h > 0) @floatFromInt(logical_h) else 1;
    return .{
        .x0 = 2.0 * @as(f32, @floatFromInt(rect.x)) / w - 1.0,
        .x1 = 2.0 * @as(f32, @floatFromInt(rect.x + rect.w)) / w - 1.0,
        .y_top = 1.0 - 2.0 * @as(f32, @floatFromInt(rect.y)) / h,
        .y_bottom = 1.0 - 2.0 * @as(f32, @floatFromInt(rect.y + rect.h)) / h,
    };
}
