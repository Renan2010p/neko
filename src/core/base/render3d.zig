// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The optional **3D** block of the backend contract.
//!
//! Only backends with a real 3D pipeline (the OpenGL backend today, Vulkan
//! next) fill this in. `Backend.render3d` stays `null` for the others, and the
//! `neko.render3d` facade no-ops, so 2D backends keep working untouched.
//!
//! Geometry is not passed across the seam: a backend owns a GPU buffer for each
//! `mesh.Kind` and only receives the kind, a model matrix and a tint per draw.

const types: type = @import("types.zig");
const mesh: type = @import("../3d/mesh.zig");
const Mat4: type = @import("../math/mat4.zig").Mat4;

pub const VTable: type = struct {
    /// Starts a 3D frame: sets the viewport/projection, enables depth testing
    /// and clears the colour + depth buffers. `view` is the world→camera matrix.
    begin3d: *const fn (ctx: *anyopaque, view: Mat4, fov_degrees: f32, near: f32, far: f32) void,
    /// Draws one built-in mesh with `model` (local→world) and a color tint.
    draw3d: *const fn (ctx: *anyopaque, kind: mesh.Kind, model: Mat4, tint: types.Color) void,
    /// Ends the 3D frame and restores the 2D state.
    end3d: *const fn (ctx: *anyopaque) void,
};
