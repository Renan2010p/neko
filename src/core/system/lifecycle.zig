//! `engine.lifecycle` — the run loop's stop condition.

const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

/// True while the window is open and no stop was requested.
pub fn keeps_running() bool {
    const e: backend.Backend = context.get() orelse return false;
    return e.keeps_running();
}

/// Asks the main loop to end after the current frame.
pub fn request_stop() void {
    const e: backend.Backend = context.get() orelse return;
    e.request_stop();
}
