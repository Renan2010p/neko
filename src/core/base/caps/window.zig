// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `window` capability: window properties, display modes and capability queries.

const std: type = @import("std");
const types: type = @import("../types.zig");

pub const VTable: type = struct {
    set_title: *const fn (ptr: *anyopaque, title: []const u8) void = noopTitle,
    set_logical_size: *const fn (ptr: *anyopaque, width: u32, height: u32) void = noopSize,
    set_fullscreen: *const fn (ptr: *anyopaque, on: bool) void = noopBool,
    set_vsync: *const fn (ptr: *anyopaque, on: bool) void = noopBool,
    set_resolution: *const fn (ptr: *anyopaque, width: u32, height: u32) void = noopSize,
    logical_size: *const fn (ptr: *anyopaque) types.Point = noopPoint,
    display_modes: *const fn (ptr: *anyopaque, allocator: std.mem.Allocator) []types.DisplayMode = noopModes,
    supports_curved_panorama: *const fn (ptr: *anyopaque) bool = noopFalse,
    supports_offscreen_targets: *const fn (ptr: *anyopaque) bool = noopFalse,
    set_draw_offset: *const fn (ptr: *anyopaque, dx: i32, dy: i32) void = noopOffset,
};

fn noopTitle(ptr: *anyopaque, title: []const u8) void {
    _ = ptr;
    _ = title;
}

fn noopSize(ptr: *anyopaque, width: u32, height: u32) void {
    _ = ptr;
    _ = width;
    _ = height;
}

fn noopBool(ptr: *anyopaque, on: bool) void {
    _ = ptr;
    _ = on;
}

fn noopOffset(ptr: *anyopaque, dx: i32, dy: i32) void {
    _ = ptr;
    _ = dx;
    _ = dy;
}

fn noopPoint(ptr: *anyopaque) types.Point {
    _ = ptr;
    return types.Point{ .x = 0, .y = 0 };
}

fn noopModes(ptr: *anyopaque, allocator: std.mem.Allocator) []types.DisplayMode {
    _ = ptr;
    _ = allocator;
    return &.{};
}

fn noopFalse(ptr: *anyopaque) bool {
    _ = ptr;
    return false;
}
