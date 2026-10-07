// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `math.Mat4` — a 4×4 `f32` matrix in **column-major** order (OpenGL layout),
//! ready to be uploaded straight to a uniform.
//!
//! Element `(row, col)` lives at `m[col * 4 + row]`. All rotations are
//! right-handed and in radians; `lookAt` produces the usual OpenGL view matrix
//! (camera looks down `-Z` in view space).

const std: type = @import("std");
const scalar: type = @import("scalar.zig");
const vec3: type = @import("vec3.zig");

const Vec3: type = vec3.Vec3;

/// A 4×4 `f32` matrix, column-major.
pub const Mat4: type = struct {
    m: [16]f32 = identity_values,

    const identity_values: [16]f32 = .{
        1, 0, 0, 0,
        0, 1, 0, 0,
        0, 0, 1, 0,
        0, 0, 0, 1,
    };

    pub const identity: Mat4 = .{};

    pub fn init(values: [16]f32) Mat4 {
        return .{ .m = values };
    }

    /// `self * other` (the usual matrix product).
    pub fn mul(self: Mat4, other: Mat4) Mat4 {
        var out: [16]f32 = undefined;
        var c: usize = 0;
        while (c < 4) : (c += 1) {
            var r: usize = 0;
            while (r < 4) : (r += 1) {
                var sum: f32 = 0;
                var k: usize = 0;
                while (k < 4) : (k += 1) {
                    sum += self.m[k * 4 + r] * other.m[c * 4 + k];
                }
                out[c * 4 + r] = sum;
            }
        }
        return .{ .m = out };
    }

    pub fn translation(offset: Vec3) Mat4 {
        return .{ .m = .{
            1,        0,        0,        0,
            0,        1,        0,        0,
            0,        0,        1,        0,
            offset.x, offset.y, offset.z, 1,
        } };
    }

    pub fn scaling(scale: Vec3) Mat4 {
        return .{ .m = .{
            scale.x, 0,       0,       0,
            0,       scale.y, 0,       0,
            0,       0,       scale.z, 0,
            0,       0,       0,       1,
        } };
    }

    pub fn rotationX(radians: f32) Mat4 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .m = .{
            1, 0,  0, 0,
            0, c,  s, 0,
            0, -s, c, 0,
            0, 0,  0, 1,
        } };
    }

    pub fn rotationY(radians: f32) Mat4 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .m = .{
            c, 0, -s, 0,
            0, 1, 0,  0,
            s, 0, c,  0,
            0, 0, 0,  1,
        } };
    }

    pub fn rotationZ(radians: f32) Mat4 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .m = .{
            c,  s, 0, 0,
            -s, c, 0, 0,
            0,  0, 1, 0,
            0,  0, 0, 1,
        } };
    }

    /// Rotation from Euler angles (radians), applied `Z * Y * X`.
    pub fn fromEuler(euler: Vec3) Mat4 {
        return rotationZ(euler.z).mul(rotationY(euler.y)).mul(rotationX(euler.x));
    }

    /// Rotation from Euler angles in degrees (`Z * Y * X`), which is what most
    /// game code and Ursina's `rotation_*` properties speak.
    pub fn fromEulerDegrees(euler: Vec3) Mat4 {
        const d2r: f32 = scalar.deg_to_rad;
        return fromEuler(.{ .x = euler.x * d2r, .y = euler.y * d2r, .z = euler.z * d2r });
    }

    /// OpenGL perspective projection (right-handed, clip Z in `[-1, 1]`).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) Mat4 {
        const f: f32 = 1.0 / @tan(fov_y * 0.5);
        const nf: f32 = 1.0 / (near - far);
        return .{ .m = .{
            f / aspect, 0, 0,                   0,
            0,          f, 0,                   0,
            0,          0, (far + near) * nf,   -1,
            0,          0, 2 * far * near * nf, 0,
        } };
    }

    /// Vulkan perspective projection (clip Z in `[0, 1]`, Y flipped because
    /// Vulkan's NDC Y points down).
    pub fn perspectiveVk(fov_y: f32, aspect: f32, near: f32, far: f32) Mat4 {
        const f: f32 = 1.0 / @tan(fov_y * 0.5);
        const nf: f32 = 1.0 / (near - far);
        return .{ .m = .{
            f / aspect, 0,  0,               0,
            0,          -f, 0,               0,
            0,          0,  far * nf,        -1,
            0,          0,  far * near * nf, 0,
        } };
    }

    /// OpenGL `lookAt` view matrix. `up` must not be parallel to the view.
    pub fn lookAt(eye: Vec3, center: Vec3, up: Vec3) Mat4 {
        const f: Vec3 = center.sub(eye).normalized();
        const s: Vec3 = f.cross(up).normalized();
        const u: Vec3 = s.cross(f);
        return .{ .m = .{
            s.x,         u.x,         -f.x,       0,
            s.y,         u.y,         -f.y,       0,
            s.z,         u.z,         -f.z,       0,
            -s.dot(eye), -u.dot(eye), f.dot(eye), 1,
        } };
    }

    pub fn transpose(self: Mat4) Mat4 {
        var out: [16]f32 = undefined;
        var c: usize = 0;
        while (c < 4) : (c += 1) {
            var r: usize = 0;
            while (r < 4) : (r += 1) {
                out[c * 4 + r] = self.m[r * 4 + c];
            }
        }
        return .{ .m = out };
    }

    /// Transforms a point (w = 1) without the perspective divide.
    pub fn transformPoint(self: Mat4, p: Vec3) Vec3 {
        return .{
            .x = self.m[0] * p.x + self.m[4] * p.y + self.m[8] * p.z + self.m[12],
            .y = self.m[1] * p.x + self.m[5] * p.y + self.m[9] * p.z + self.m[13],
            .z = self.m[2] * p.x + self.m[6] * p.y + self.m[10] * p.z + self.m[14],
        };
    }

    /// Transforms a direction (w = 0), ignoring translation.
    pub fn transformDirection(self: Mat4, v: Vec3) Vec3 {
        return .{
            .x = self.m[0] * v.x + self.m[4] * v.y + self.m[8] * v.z,
            .y = self.m[1] * v.x + self.m[5] * v.y + self.m[9] * v.z,
            .z = self.m[2] * v.x + self.m[6] * v.y + self.m[10] * v.z,
        };
    }

    /// Full 4-component transform, useful for clip-space points.
    pub fn transformVec4(self: Mat4, v: [4]f32) [4]f32 {
        return .{
            self.m[0] * v[0] + self.m[4] * v[1] + self.m[8] * v[2] + self.m[12] * v[3],
            self.m[1] * v[0] + self.m[5] * v[1] + self.m[9] * v[2] + self.m[13] * v[3],
            self.m[2] * v[0] + self.m[6] * v[1] + self.m[10] * v[2] + self.m[14] * v[3],
            self.m[3] * v[0] + self.m[7] * v[1] + self.m[11] * v[2] + self.m[15] * v[3],
        };
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Mat4 identity leaves points alone" {
    const p: Vec3 = Mat4.identity.transformPoint(Vec3.init(1, 2, 3));
    try testing.expectApproxEqAbs(@as(f32, 1), p.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 2), p.y, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 3), p.z, 0.0001);
}

test "Mat4 translation then scale" {
    const m: Mat4 = Mat4.translation(Vec3.init(10, 0, 0)).mul(Mat4.scaling(Vec3.splat(2)));
    const p: Vec3 = m.transformPoint(Vec3.init(1, 1, 1));
    try testing.expectApproxEqAbs(@as(f32, 12), p.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 2), p.y, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 2), p.z, 0.0001);
}

test "Mat4 rotationZ turns +X into +Y" {
    const p: Vec3 = Mat4.rotationZ(std.math.pi / 2.0).transformPoint(Vec3.right);
    try testing.expectApproxEqAbs(@as(f32, 0), p.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 1), p.y, 0.0001);
}

test "Mat4 lookAt puts the target straight ahead" {
    const view: Mat4 = Mat4.lookAt(Vec3.init(0, 0, -20), Vec3.zero, Vec3.up);
    const p: Vec3 = view.transformPoint(Vec3.zero);
    // The origin should end up on the -Z axis (in front of the camera).
    try testing.expectApproxEqAbs(@as(f32, 0), p.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 0), p.y, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, -20), p.z, 0.0001);
}

test "Mat4 perspective maps the near plane to -1" {
    const proj: Mat4 = Mat4.perspective(std.math.pi / 3.0, 1.0, 1.0, 100.0);
    const clip: [4]f32 = proj.transformVec4(.{ 0, 0, -1, 1 });
    try testing.expectApproxEqAbs(@as(f32, -1), clip[2] / clip[3], 0.0001);
}
