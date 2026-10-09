// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `core` capability: lifecycle, presentation and timing.
//!
//! Every runnable backend implements this. The defaults are inert so that a
//! backend that forgets to fill a field fails cleanly (`init` returns `false`)
//! instead of doing something surprising.

const types: type = @import("../types.zig");

pub const VTable: type = struct {
    init: *const fn (ptr: *anyopaque, config: types.Config) bool = noopInit,
    shutdown: *const fn (ptr: *anyopaque) void = noopVoid,
    keeps_running: *const fn (ptr: *anyopaque) bool = noopFalse,
    request_stop: *const fn (ptr: *anyopaque) void = noopVoid,
    present: *const fn (ptr: *anyopaque) void = noopVoid,
    ticks_ms: *const fn (ptr: *anyopaque) u64 = noopZero,
};

fn noopInit(ptr: *anyopaque, config: types.Config) bool {
    _ = ptr;
    _ = config;
    return false;
}

fn noopVoid(ptr: *anyopaque) void {
    _ = ptr;
}

fn noopFalse(ptr: *anyopaque) bool {
    _ = ptr;
    return false;
}

fn noopZero(ptr: *anyopaque) u64 {
    _ = ptr;
    return 0;
}
