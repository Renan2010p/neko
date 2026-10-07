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
//!
//! This is the public facade; the implementation lives in `math/scalar.zig`
//! and `math/vec2.zig`.

const scalar: type = @import("math/scalar.zig");
const vec2: type = @import("math/vec2.zig");
const vec3: type = @import("math/vec3.zig");
const mat4: type = @import("math/mat4.zig");

// Types.
pub const Vec2: type = vec2.Vec2;
pub const Vec3: type = vec3.Vec3;
pub const Mat4: type = mat4.Mat4;

// Constants.
pub const rad_to_deg: f32 = scalar.rad_to_deg;
pub const deg_to_rad: f32 = scalar.deg_to_rad;

// Scalars.
pub const clamp: *const fn (f32, f32, f32) f32 = scalar.clamp;
pub const clamp01: *const fn (f32) f32 = scalar.clamp01;
pub const lerp: *const fn (f32, f32, f32) f32 = scalar.lerp;
pub const inverseLerp: *const fn (f32, f32, f32) f32 = scalar.inverseLerp;
pub const smoothstep: *const fn (f32, f32, f32) f32 = scalar.smoothstep;
pub const moveTowards: *const fn (f32, f32, f32) f32 = scalar.moveTowards;
pub const damp: *const fn (f32, f32, f32, f32) f32 = scalar.damp;
pub const sign: *const fn (f32) f32 = scalar.sign;
pub const wrap: *const fn (f32, f32, f32) f32 = scalar.wrap;
pub const toRadians: *const fn (f32) f32 = scalar.toRadians;
pub const toDegrees: *const fn (f32) f32 = scalar.toDegrees;
