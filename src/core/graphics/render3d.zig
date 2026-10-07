// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.render3d` — the public 3D entry point.
//!
//! Game code builds a `Camera`, then calls `begin` once, `draw` per entity and
//! `end` once per frame. Jobs are forwarded to the backend's optional 3D
//! pipeline; on a 2D-only backend every call is a no-op (check `supported`).
//!
//! ```
//! var cam = neko.Camera.init();
//! cam.position = .{ .z = -12 };
//! neko.render3d.begin(cam);
//! neko.render3d.draw(.cube, neko.Mat4.translation(.{ .y = 0 }), neko.Color.white);
//! neko.render3d.end();
//! ```

const context: type = @import("../base/context.zig");
const mesh: type = @import("../3d/mesh.zig");
const camera_mod: type = @import("../3d/camera.zig");
const types: type = @import("../base/types.zig");
const Mat4: type = @import("../math/mat4.zig").Mat4;

pub const Kind: type = mesh.Kind;
pub const Vertex: type = mesh.Vertex;
pub const Camera: type = camera_mod.Camera;

/// True when the active backend implements the 3D pipeline.
pub fn supported() bool {
    const b = context.get() orelse return false;
    return b.render3d != null;
}

/// Starts a 3D frame for `camera`: projection, depth test and buffer clear.
pub fn begin(camera: Camera) void {
    const b = context.get() orelse return;
    const vt = b.render3d orelse return;
    vt.begin3d(b.ptr, camera.view(), camera.fov, camera.near, camera.far);
}

/// Draws a built-in mesh with a local→world `model` matrix and a color tint.
pub fn draw(kind: Kind, model: Mat4, tint: types.Color) void {
    const b = context.get() orelse return;
    const vt = b.render3d orelse return;
    vt.draw3d(b.ptr, kind, model, tint);
}

/// Ends the 3D frame and restores the 2D pipeline state.
pub fn end() void {
    const b = context.get() orelse return;
    const vt = b.render3d orelse return;
    vt.end3d(b.ptr);
}
