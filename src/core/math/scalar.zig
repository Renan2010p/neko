// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `math` scalar helpers — allocation-free and pure.

const std: type = @import("std");

/// Radians to degrees.
pub const rad_to_deg: f32 = 180.0 / std.math.pi;
/// Degrees to radians.
pub const deg_to_rad: f32 = std.math.pi / 180.0;

/// Restricts `v` to `[min, max]`.
pub fn clamp(v: f32, min: f32, max: f32) f32 {
    return @max(min, @min(max, v));
}

/// Restricts `v` to `[0, 1]`.
pub fn clamp01(v: f32) f32 {
    return @max(0.0, @min(1.0, v));
}

/// Linear interpolation: `a` at `t = 0`, `b` at `t = 1` (t clamped).
pub fn lerp(a: f32, b: f32, t: f32) f32 {
    return a + (b - a) * clamp01(t);
}

/// The inverse of `lerp`: where `v` sits between `a` and `b` (0..1).
pub fn inverseLerp(a: f32, b: f32, v: f32) f32 {
    if (a == b) return 0;
    return clamp01((v - a) / (b - a));
}

/// A smooth 0..1 ramp using the classic Hermite curve.
pub fn smoothstep(edge0: f32, edge1: f32, v: f32) f32 {
    const t: f32 = inverseLerp(edge0, edge1, v);
    return t * t * (3.0 - 2.0 * t);
}

/// Moves `value` toward `target` by at most `max_delta`.
pub fn moveTowards(value: f32, target: f32, max_delta: f32) f32 {
    const diff: f32 = target - value;
    if (@abs(diff) <= max_delta) return target;
    return value + @as(f32, if (diff > 0) 1 else -1) * max_delta;
}

/// Eases `value` toward `target` frame-rate-independently.
/// `smoothing` is the fraction remaining after one second (0..1).
pub fn damp(value: f32, target: f32, smoothing: f32, dt: f32) f32 {
    const s: f32 = clamp01(smoothing);
    return lerp(target, value, std.math.pow(f32, s, dt));
}

/// `-1`, `0` or `1` depending on the sign of `v`.
pub fn sign(v: f32) f32 {
    if (v > 0) return 1;
    if (v < 0) return -1;
    return 0;
}

/// Wraps `v` into `[min, max)`.
pub fn wrap(v: f32, min: f32, max: f32) f32 {
    const range: f32 = max - min;
    if (range == 0) return min;
    var r: f32 = @mod(v - min, range);
    if (r < 0) r += range;
    return r + min;
}

/// Converts degrees to radians.
pub fn toRadians(degrees: f32) f32 {
    return degrees * deg_to_rad;
}

/// Converts radians to degrees.
pub fn toDegrees(radians: f32) f32 {
    return radians * rad_to_deg;
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "scalar helpers" {
    try testing.expectApproxEqAbs(@as(f32, 5), clamp(10, 0, 5), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 0.5), inverseLerp(0, 10, 5), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 3), moveTowards(0, 10, 3), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 2), wrap(5, 0, 3), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 1), sign(3.2), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, -1), sign(-0.1), 0.0001);
}
