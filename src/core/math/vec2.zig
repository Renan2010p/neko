// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `math.Vec2` — a 2D vector of `f32`.

const std: type = @import("std");
const scalar: type = @import("scalar.zig");
const types: type = @import("../base/types.zig");

/// A 2D vector of `f32`. Used for positions, sizes, velocities and directions.
pub const Vec2: type = struct {
    x: f32 = 0,
    y: f32 = 0,

    pub const zero: Vec2 = .{ .x = 0, .y = 0 };
    pub const one: Vec2 = .{ .x = 1, .y = 1 };
    pub const up: Vec2 = .{ .x = 0, .y = -1 };
    pub const down: Vec2 = .{ .x = 0, .y = 1 };
    pub const left: Vec2 = .{ .x = -1, .y = 0 };
    pub const right: Vec2 = .{ .x = 1, .y = 0 };

    pub fn init(x: f32, y: f32) Vec2 {
        return .{ .x = x, .y = y };
    }

    pub fn add(self: Vec2, other: Vec2) Vec2 {
        return .{ .x = self.x + other.x, .y = self.y + other.y };
    }

    pub fn sub(self: Vec2, other: Vec2) Vec2 {
        return .{ .x = self.x - other.x, .y = self.y - other.y };
    }

    pub fn mul(self: Vec2, other: Vec2) Vec2 {
        return .{ .x = self.x * other.x, .y = self.y * other.y };
    }

    pub fn scale(self: Vec2, factor: f32) Vec2 {
        return .{ .x = self.x * factor, .y = self.y * factor };
    }

    pub fn dot(self: Vec2, other: Vec2) f32 {
        return self.x * other.x + self.y * other.y;
    }

    /// The squared length. Cheaper than `length`; use it for comparisons.
    pub fn lengthSquared(self: Vec2) f32 {
        return self.dot(self);
    }

    pub fn length(self: Vec2) f32 {
        return @sqrt(self.lengthSquared());
    }

    /// Distance to `other`.
    pub fn distance(self: Vec2, other: Vec2) f32 {
        return self.sub(other).length();
    }

    /// A unit vector in the same direction, or `zero` when this is `zero`.
    pub fn normalized(self: Vec2) Vec2 {
        const len: f32 = self.length();
        if (len <= 0.000001) return .zero;
        return self.scale(1.0 / len);
    }

    /// Rotates the vector by `radians` around the origin.
    pub fn rotated(self: Vec2, radians: f32) Vec2 {
        const c: f32 = @cos(radians);
        const s: f32 = @sin(radians);
        return .{ .x = self.x * c - self.y * s, .y = self.x * s + self.y * c };
    }

    /// The heading of the vector in radians (`atan2`).
    pub fn angle(self: Vec2) f32 {
        return std.math.atan2(self.y, self.x);
    }

    /// Clamps both components into `[min, max]`.
    pub fn clamp(self: Vec2, min: f32, max: f32) Vec2 {
        return .{ .x = @max(min, @min(max, self.x)), .y = @max(min, @min(max, self.y)) };
    }

    /// Linearly interpolates toward `other`. `t` is clamped to 0..1.
    pub fn lerp(self: Vec2, other: Vec2, t: f32) Vec2 {
        const c: f32 = scalar.clamp01(t);
        return .{ .x = self.x + (other.x - self.x) * c, .y = self.y + (other.y - self.y) * c };
    }

    /// Converts to an integer pixel point (truncating toward zero).
    pub fn toPoint(self: Vec2) types.Point {
        return .{ .x = @intFromFloat(self.x), .y = @intFromFloat(self.y) };
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Vec2 length and normalize" {
    const v: Vec2 = Vec2.init(3, 4);
    try testing.expectApproxEqAbs(@as(f32, 5), v.length(), 0.0001);
    const n: Vec2 = v.normalized();
    try testing.expectApproxEqAbs(@as(f32, 1), n.length(), 0.0001);
}

test "Vec2 lerp and clamp" {
    const v: Vec2 = Vec2.zero.lerp(Vec2.init(10, 20), 0.5);
    try testing.expectApproxEqAbs(@as(f32, 5), v.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 10), v.y, 0.0001);
    const c: Vec2 = Vec2.init(5, -5).clamp(0, 1);
    try testing.expectApproxEqAbs(@as(f32, 1), c.x, 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 0), c.y, 0.0001);
}
