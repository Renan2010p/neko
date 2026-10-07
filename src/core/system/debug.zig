//! `neko.debug` — frame diagnostics.
//!
//! Call `tick()` once per frame, then `draw_fps(x, y, color)` to draw an
//! "N FPS" label.

const std: type = @import("std");
const fmt: type = std.fmt;
const types: type = @import("../base/types.zig");
const sprite: type = @import("../graphics/sprite.zig");
const time: type = @import("../system/time.zig");

const SAMPLE_MS: u64 = 500;

var frames: u32 = 0;
var accum_ms: u64 = 0;
var last_ms: u64 = 0;
var started: bool = false;
var fps_value: u32 = 0;

/// Advances the counter. Call once per frame.
pub fn tick() void {
    const now: u64 = time.ticks_ms();
    if (!started) {
        started = true;
        last_ms = now;
        return;
    }

    accum_ms += now - last_ms;
    last_ms = now;
    frames += 1;

    if (accum_ms >= SAMPLE_MS) {
        fps_value = @intCast((frames * 1000) / accum_ms);
        frames = 0;
        accum_ms = 0;
    }
}

/// The last measured frames per second.
pub fn fps() u32 {
    return fps_value;
}

/// Draws "N FPS" at (x, y) in `color`.
pub fn draw_fps(x: i32, y: i32, color: types.Color) void {
    var buf: [16]u8 = undefined;
    const label: []u8 = fmt.bufPrint(&buf, "{d} FPS", .{fps_value}) catch return;
    sprite.text(label, x, y, sprite.Options{ .size = 20, .color = color });
}
