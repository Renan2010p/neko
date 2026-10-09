// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `input` capability: the event queue and live pointer state.

const types: type = @import("../types.zig");
const event: type = @import("../../system/event.zig");

pub const VTable: type = struct {
    poll_event: *const fn (ptr: *anyopaque) ?event.Event = noopPoll,
    mouse_pos: *const fn (ptr: *anyopaque) types.Point = noopPoint,
};

fn noopPoll(ptr: *anyopaque) ?event.Event {
    _ = ptr;
    return null;
}

fn noopPoint(ptr: *anyopaque) types.Point {
    _ = ptr;
    return types.Point{ .x = 0, .y = 0 };
}
