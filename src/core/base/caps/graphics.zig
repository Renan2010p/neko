// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `graphics` capability: 2D primitives, textures and render targets.

const types: type = @import("../types.zig");

pub const VTable: type = struct {
    clear: *const fn (ptr: *anyopaque, color: types.Color) void = noopColor,
    draw_rect: *const fn (ptr: *anyopaque, rect: types.Rect, color: types.Color, filled: bool) void = noopRect,
    draw_line: *const fn (ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: types.Color) void = noopLine,
    draw_circle: *const fn (ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: types.Color, filled: bool) void = noopCircle,
    load_texture: *const fn (ptr: *anyopaque, path: []const u8) ?types.TextureHandle = noopNullTexture,
    create_target: *const fn (ptr: *anyopaque, width: u32, height: u32) ?types.TextureHandle = noopSizeTexture,
    create_texture: *const fn (ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?types.TextureHandle = noopCreateTexture,
    update_texture: *const fn (ptr: *anyopaque, tex: types.TextureHandle, pixels: []const u8, pitch: u32) void = noopUpdateTexture,
    draw_texture: *const fn (ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void = noopDrawTexture,
    draw_texture_rotated: *const fn (ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void = noopDrawTextureRotated,
    texture_size: *const fn (ptr: *anyopaque, tex: types.TextureHandle) types.Point = noopTextureSize,
    geometry: *const fn (ptr: *anyopaque, tex: types.TextureHandle, vertices: []const types.Vertex, indices: []const i32) void = noopGeometry,
    set_render_target: *const fn (ptr: *anyopaque, target: ?types.TextureHandle) void = noopTarget,
};

fn noopColor(ptr: *anyopaque, color: types.Color) void {
    _ = ptr;
    _ = color;
}

fn noopRect(ptr: *anyopaque, rect: types.Rect, color: types.Color, filled: bool) void {
    _ = ptr;
    _ = rect;
    _ = color;
    _ = filled;
}

fn noopLine(ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: types.Color) void {
    _ = ptr;
    _ = x1;
    _ = y1;
    _ = x2;
    _ = y2;
    _ = color;
}

fn noopCircle(ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: types.Color, filled: bool) void {
    _ = ptr;
    _ = cx;
    _ = cy;
    _ = radius;
    _ = color;
    _ = filled;
}

fn noopNullTexture(ptr: *anyopaque, path: []const u8) ?types.TextureHandle {
    _ = ptr;
    _ = path;
    return null;
}

fn noopSizeTexture(ptr: *anyopaque, width: u32, height: u32) ?types.TextureHandle {
    _ = ptr;
    _ = width;
    _ = height;
    return null;
}

fn noopCreateTexture(ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?types.TextureHandle {
    _ = ptr;
    _ = width;
    _ = height;
    _ = pixels;
    _ = pitch;
    return null;
}

fn noopUpdateTexture(ptr: *anyopaque, tex: types.TextureHandle, pixels: []const u8, pitch: u32) void {
    _ = ptr;
    _ = tex;
    _ = pixels;
    _ = pitch;
}

fn noopDrawTexture(ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void {
    _ = ptr;
    _ = tex;
    _ = dst;
    _ = src;
    _ = alpha;
}

fn noopDrawTextureRotated(ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void {
    _ = ptr;
    _ = tex;
    _ = dst;
    _ = angle;
    _ = alpha;
}

fn noopTextureSize(ptr: *anyopaque, tex: types.TextureHandle) types.Point {
    _ = ptr;
    _ = tex;
    return types.Point{ .x = 0, .y = 0 };
}

fn noopGeometry(ptr: *anyopaque, tex: types.TextureHandle, vertices: []const types.Vertex, indices: []const i32) void {
    _ = ptr;
    _ = tex;
    _ = vertices;
    _ = indices;
}

fn noopTarget(ptr: *anyopaque, target: ?types.TextureHandle) void {
    _ = ptr;
    _ = target;
}
