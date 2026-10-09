// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `text` capability: font loading and text drawing.

const types: type = @import("../types.zig");

pub const VTable: type = struct {
    load_font: *const fn (ptr: *anyopaque, path: []const u8, size: u16) i64 = noopFont,
    draw_text: *const fn (ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: types.Color, center: bool, font_idx: i64) bool = noopDrawText,
    draw_text_rotated: *const fn (ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: types.Color, center: bool, font_idx: i64) bool = noopDrawTextRotated,
    text_size: *const fn (ptr: *anyopaque, text: []const u8, font_idx: u32) ?types.Point = noopTextSize,
};

fn noopFont(ptr: *anyopaque, path: []const u8, size: u16) i64 {
    _ = ptr;
    _ = path;
    _ = size;
    return -1;
}

fn noopDrawText(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: types.Color, center: bool, font_idx: i64) bool {
    _ = ptr;
    _ = text;
    _ = x;
    _ = y;
    _ = size;
    _ = color;
    _ = center;
    _ = font_idx;
    return false;
}

fn noopDrawTextRotated(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: types.Color, center: bool, font_idx: i64) bool {
    _ = ptr;
    _ = text;
    _ = x;
    _ = y;
    _ = size;
    _ = angle;
    _ = color;
    _ = center;
    _ = font_idx;
    return false;
}

fn noopTextSize(ptr: *anyopaque, text: []const u8, font_idx: u32) ?types.Point {
    _ = ptr;
    _ = text;
    _ = font_idx;
    return null;
}
