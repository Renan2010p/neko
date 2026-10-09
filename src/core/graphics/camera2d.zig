// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.Camera2D` — a 2D camera that scrolls the world.
//!
//! It works by shifting the core's draw calls, not through a backend feature,
//! so it behaves identically on every backend (SDL2, PS2, …). While a camera is
//! active, world draws are translated; UI typically draws after `end()`.
//!
//! ```zig
//! var cam = neko.Camera2D{ .position = .{ .x = 100, .y = 40 } };
//! cam.begin();
//! // ... draw the world ...
//! cam.end();
//! // ... draw the HUD ...
//! ```

const types: type = @import("../base/types.zig");

/// A 2D camera. `position` is the world point drawn at the screen's top-left.
pub const Camera2D: type = struct {
    position: types.Point = types.Point{ .x = 0, .y = 0 },

    /// Starts drawing relative to this camera.
    pub fn begin(self: *const Camera2D) void {
        setPosition(self.position);
    }

    /// Stops drawing relative to a camera (UI/debug).
    pub fn end(self: *const Camera2D) void {
        _ = self;
        clear();
    }
};

var offset_value: types.Point = types.Point{ .x = 0, .y = 0 };
var is_active: bool = false;

/// The translation the core wrappers apply to world draws (0,0 when inactive).
pub fn offset() types.Point {
    return offset_value;
}

/// True while a camera is active.
pub fn active() bool {
    return is_active;
}

/// Makes `position` the world point at the screen's top-left.
pub fn setPosition(position: types.Point) void {
    offset_value = types.Point{ .x = -position.x, .y = -position.y };
    is_active = true;
}

/// Turns the camera off.
pub fn clear() void {
    offset_value = types.Point{ .x = 0, .y = 0 };
    is_active = false;
}

/// Applies the current offset to a point.
pub fn apply(point: types.Point) types.Point {
    return types.Point{ .x = point.x + offset_value.x, .y = point.y + offset_value.y };
}

/// Applies the current offset to a rectangle.
pub fn applyRect(rect: types.Rect) types.Rect {
    return types.Rect{
        .x = rect.x + offset_value.x,
        .y = rect.y + offset_value.y,
        .w = rect.w,
        .h = rect.h,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Camera2D offsets points and resets" {
    clear();
    try testing.expectEqual(@as(i32, 0), offset().x);

    setPosition(.{ .x = 10, .y = 20 });
    try testing.expectEqual(@as(i32, -10), offset().x);
    try testing.expectEqual(@as(i32, 5), apply(.{ .x = 15, .y = 0 }).x);

    clear();
    try testing.expect(!active());
}
