//! `neko.math` — small, allocation-free helpers for game code.
//!
//! Nothing here touches the backend or the heap, so it works on every platform
//! (including the PS2). It is deliberately tiny: `Vec2` for floating-point
//! positions/velocities and a handful of scalar functions that show up in every
//! game loop.
//!
//! ```
//! const m = neko.math;
//! var pos = m.Vec2{ .x = 0, .y = 0 };
//! pos = pos.add(m.Vec2{ .x = 10, .y = 0 }.scale(dt));
//! const t = m.clamp01(elapsed / duration);
//! const eased = m.smoothstep(0, 1, t);
//! ```

const std: type = @import("std");

/// Radians to degrees.
pub const rad_to_deg: f32 = 180.0 / std.math.pi;
/// Degrees to radians.
pub const deg_to_rad: f32 = std.math.pi / 180.0;

// ── Vec2 ─────────────────────────────────────────────────────────────────────

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
        const c: f32 = clamp01(t);
        return .{ .x = self.x + (other.x - self.x) * c, .y = self.y + (other.y - self.y) * c };
    }

    /// Converts to an integer pixel point (truncating toward zero).
    pub fn toPoint(self: Vec2) @import("types.zig").Point {
        return .{ .x = @intFromFloat(self.x), .y = @intFromFloat(self.y) };
    }
};

// ── Scalars ──────────────────────────────────────────────────────────────────

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

test "scalar helpers" {
    try testing.expectApproxEqAbs(@as(f32, 5), clamp(10, 0, 5), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 0.5), inverseLerp(0, 10, 5), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 3), moveTowards(0, 10, 3), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 2), wrap(5, 0, 3), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, 1), sign(3.2), 0.0001);
    try testing.expectApproxEqAbs(@as(f32, -1), sign(-0.1), 0.0001);
}
