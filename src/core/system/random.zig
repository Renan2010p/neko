//! `neko.random` — gameplay randomness.
//!
//! A process-wide PRNG. It seeds itself lazily from its own ASLR address (so
//! runs differ) and can be re-seeded explicitly with `seed`.

const std: type = @import("std");
const Random: type = std.Random;
const DefaultPrng: type = std.Random.DefaultPrng;

var prng: DefaultPrng = undefined;
var ready: bool = false;

fn rng() Random {
    if (!ready) {
        prng = DefaultPrng.init(@intFromPtr(&prng));
        ready = true;
    }
    return prng.random();
}

/// Re-seeds the generator.
pub fn seed(value: u64) void {
    prng = DefaultPrng.init(value);
    ready = true;
}

/// A fair coin flip.
pub fn boolean() bool {
    return rng().boolean();
}

/// True with probability `chance` (0.0 = never, 1.0 = always).
pub fn probability(chance: f32) bool {
    return rng().float(f32) < chance;
}

/// A uniform integer in `[min, max]`.
pub fn int_range(min: i32, max: i32) i32 {
    return rng().intRangeAtMost(i32, min, max);
}

/// A uniform float in `[min, max)`.
pub fn float_range(min: f32, max: f32) f32 {
    return min + rng().float(f32) * (max - min);
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "random: seeding makes sequences reproducible" {
    seed(42);
    const a: i32 = int_range(-100, 100);
    const b: bool = boolean();
    const c: f32 = float_range(0, 1);

    seed(42);
    try testing.expectEqual(a, int_range(-100, 100));
    try testing.expectEqual(b, boolean());
    try testing.expectApproxEqAbs(c, float_range(0, 1), 0.0001);
}

test "random: int_range stays in bounds" {
    seed(1);
    var i: usize = 0;
    while (i < 1000) : (i += 1) {
        const v: i32 = int_range(3, 7);
        try testing.expect(v >= 3 and v <= 7);
    }
}
