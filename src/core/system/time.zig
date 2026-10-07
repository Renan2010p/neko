//! `neko.time` — monotonic clock and the current frame's delta time.

const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

var current_dt: f32 = 0;

/// Milliseconds since the engine started, monotonic.
pub fn ticks_ms() u64 {
    const e: backend.Backend = context.get() orelse return 0;
    return e.ticks_ms();
}

/// Seconds since the previous frame, as computed by `neko.run` / `neko.app`.
/// Handy in helpers that do not receive `dt` directly.
pub fn dt() f32 {
    return current_dt;
}

/// Seconds since startup, approximated from `ticks_ms`.
pub fn elapsed() f32 {
    return @as(f32, @floatFromInt(ticks_ms())) / 1000.0;
}

/// Internal: the runner stores the current frame's delta.
pub fn setDt(value: f32) void {
    current_dt = value;
}
