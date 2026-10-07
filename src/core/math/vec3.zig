// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `math.Vec3` — a 3D vector of `f32`, the base of every 3D transform.
//!
//! Neko follows Ursina's world convention: **X right, Y up, Z forward**,
//! right-handed (`X × Y = Z`). `Vec3.up` is therefore `(0, 1, 0)` and the
//! default camera sits at `(0, 0, -20)` looking toward `+Z`.

const std: type = @import("std");
const scalar: type = @import("scalar.zig");
const vec2: type = @import("vec2.zig");

/// A 3D vector of `f32`. Positions, directions, scales and Euler angles.
pub const Vec3: type = struct {
    x: f32 = 0,
    y: f32 = 0,
    z: f32 = 0,

    pub const zero: Vec3 = .{ .x = 0, .y = 0, .z = 0 };
    pub const one: Vec3 = .{ .x = 1, .y = 1, .z = 1 };
    pub const up: Vec3 = .{ .x = 0, .y = 1, .z = 0 };
    pub const down: Vec3 = .{ .x = 0, .y = -1, .z = 0 };
    pub const left: Vec3 = .{ .x = -1, .y = 0, .z = 0 };
    pub const right: Vec3 = .{ .x = 1, .y = 0, .z = 0 };
    pub const forward: Vec3 = .{ .x = 0, .y = 0, .z = 1 };
    pub const back: Vec3 = .{ .x = 0, .y = 0, .z = -1 };

    pub fn init(x: f32, y: f32, z: f32) Vec3 {
        return .{ .x = x, .y = y, .z = z };
    }

    /// Fills all three components with `v` (Ursina's `Vec3(1)`).
    pub fn splat(v: f32) Vec3 {
        return .{ .x = v, .y = v, .z = v };
    }

    pub fn add(self: Vec3, other: Vec3) Vec3 {
        return .{ .x = self.x + other.x, .y = self.y + other.y, .z = self.z + other.z };
    }

    pub fn sub(self: Vec3, other: Vec3) Vec3 {
        return .{ .x = self.x - other.x, .y = self.y - other.y, .z = self.z - other.z };
    }

    pub fn mul(self: Vec3, other: Vec3) Vec3 {
        return .{ .x = self.x * other.x, .y = self.y * other.y, .z = self.z * other.z };
    }

    pub fn scale(self: Vec3, factor: f32) Vec3 {
        return .{ .x = self.x * factor, .y = self.y * factor, .z = self.z * factor };
    }

    /// Component-wise division. Components of `other` that are zero are left
    /// untouched (Ursina guards scale against zero the same way).
    pub fn div(self: Vec3, other: Vec3) Vec3 {
        return .{
            .x = if (other.x != 0) self.x / other.x else self.x,
            .y = if (other.y != 0) self.y / other.y else self.y,
            .z = if (other.z != 0) self.z / other.z else self.z,
        };
    }

    pub fn negated(self: Vec3) Vec3 {
        return .{ .x = -self.x, .y = -self.y, .z = -self.z };
    }

    pub fn dot(self: Vec3, other: Vec3) f32 {
        return self.x * other.x + self.y * other.y + self.z * other.z;
    }

    /// Right-handed cross product (`X × Y = Z`).
    pub fn cross(self: Vec3, other: Vec3) Vec3 {
        return .{
            .x = self.y * other.z - self.z * other.y,
            .y = self.z * other.x - self.x * other.z,
            .z = self.x * other.y - self.y * other.x,
        };
    }

    pub fn lengthSquared(self: Vec3) f32 {
        return self.dot(self);
    }

    pub fn length(self: Vec3) f32 {
        return @sqrt(self.lengthSquared());
    }

    pub fn distance(self: Vec3, other: Vec3) f32 {
        return self.sub(other).length();
    }

    /// A unit vector in the same direction, or `zero` when this is `zero`.
    pub fn normalized(self: Vec3) Vec3 {
        const len: f32 = self.length();
        if (len <= 0.000001) return .zero;
        return self.scale(1.0 / len);
    }

    pub fn lerp(self: Vec3, other: Vec3, t: f32) Vec3 {
        const c: f32 = scalar.clamp01(t);
        return .{
            .x = self.x + (other.x - self.x) * c,
            .y = self.y + (other.y - self.y) * c,
            .z = self.z + (other.z - self.z) * c,
        };
    }

    pub fn clamp(self: Vec3, min: f32, max: f32) Vec3 {
        return .{
            .x = @max(min, @min(max, self.x)),
            .y = @max(min, @min(max, self.y)),
            .z = @max(min, @min(max, self.z)),
        };
    }

    /// Rotates around the X axis by `radians` (right-handed).
    pub fn rotatedX(self: Vec3, radians: f32) Vec3 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .x = self.x, .y = self.y * c - self.z * s, .z = self.y * s + self.z * c };
    }

    pub fn rotatedY(self: Vec3, radians: f32) Vec3 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .x = self.x * c + self.z * s, .y = self.y, .z = -self.x * s + self.z * c };
    }

    pub fn rotatedZ(self: Vec3, radians: f32) Vec3 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .x = self.x * c - self.y * s, .y = self.x * s + self.y * c, .z = self.z };
    }

    /// Drops Z, giving a `Vec2`.
    pub fn toVec2(self: Vec3) vec2.Vec2 {
        return .{ .x = self.x, .y = self.y };
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Vec3 length and normalize" {
    const v: Vec3 = Vec3.init(2, 3, 6);
    try testing.expectApproxEqAbs(@as(f32, 7), v.length(), 0.0001);
    const n: Vec3 = v.normalized();
    try testing.expectApproxEqAbs(@as(f32, 1), n.length(), 0.0001);
    try testing.expect(n.normalized().sub(n).length() <= 0.0001);
}

test "Vec3 cross is right-handed" {
    const z: Vec3 = Vec3.right.cross(Vec3.up);
    try testing.expectApproxEqAbs(@as(f32, 0), z.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 0), z.y, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 1), z.z, 0.0001);
}

test "Vec3 rotations keep length" {
    const v: Vec3 = Vec3.init(1, 0, 0);
    try testing.expectApproxEqAbs(@as(f32, 1), v.rotatedZ(std.math.pi / 2.0).length(), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 1), v.rotatedY(std.math.pi / 2.0).length(), 0.0001);
}
