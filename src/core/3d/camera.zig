// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.Camera` — the 3D camera, in Ursina's convention.
//!
//! Position is a `Vec3`, rotation is Euler degrees (`x` pitch, `y` yaw, `z`
//! roll) applied `Z*Y*X`, and the camera looks down its local `+Z`. With the
//! default transform it sits at `(0, 0, -20)` looking toward the origin, so
//! entities spawn in front of it at `z ≈ 0`.

const std: type = @import("std");
const vec3: type = @import("../math/vec3.zig");
const mat4: type = @import("../math/mat4.zig");

const Vec3: type = vec3.Vec3;
const Mat4: type = mat4.Mat4;
const deg_to_rad: f32 = std.math.pi / 180.0;

pub const Camera: type = struct {
    position: Vec3 = .{ .x = 0, .y = 0, .z = -20 },
    /// Euler angles in degrees.
    rotation: Vec3 = .zero,
    /// Vertical field of view in degrees (Ursina's default is 40).
    fov: f32 = 40,
    near: f32 = 0.1,
    far: f32 = 1000,

    pub fn init() Camera {
        return .{};
    }

    /// The world-space direction the camera is looking along.
    pub fn forward(self: Camera) Vec3 {
        const rot: Mat4 = Mat4.fromEulerDegrees(self.rotation);
        return rot.transformDirection(Vec3.forward).normalized();
    }

    pub fn up(self: Camera) Vec3 {
        const rot: Mat4 = Mat4.fromEulerDegrees(self.rotation);
        return rot.transformDirection(Vec3.up).normalized();
    }

    /// The OpenGL view matrix (world → camera space).
    pub fn view(self: Camera) Mat4 {
        return Mat4.lookAt(self.position, self.position.add(self.forward()), self.up());
    }

    /// The OpenGL projection matrix for the given viewport `aspect`.
    pub fn projection(self: Camera, aspect: f32) Mat4 {
        const a: f32 = if (aspect > 0) aspect else 1.0;
        return Mat4.perspective(self.fov * deg_to_rad, a, self.near, self.far);
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "default camera looks toward the origin from -Z" {
    const cam: Camera = Camera.init();
    const eye_space: Vec3 = cam.view().transformPoint(Vec3.zero);
    try testing.expect(eye_space.z < 0); // the origin is in front, along -Z
    const clip: [4]f32 = cam.projection(1.0).transformVec4(.{ eye_space.x, eye_space.y, eye_space.z, 1 });
    try testing.expect(clip[3] > 0);
}

test "camera forward follows yaw" {
    var cam: Camera = Camera.init();
    cam.rotation.y = 90;
    const f: Vec3 = cam.forward();
    try testing.expectApproxEqAbs(@as(f32, 1), f.x, 0.001);
}
