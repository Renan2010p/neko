// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `files` capability: file access owned by the platform.
//!
//! The core never touches the host filesystem; save files and assets go
//! through these entries. A backend without a filesystem leaves them at their
//! defaults, so saves degrade to "no save file" instead of failing to compile.

const std: type = @import("std");

pub const VTable: type = struct {
    read_file: *const fn (ptr: *anyopaque, allocator: std.mem.Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 = noopRead,
    write_file: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool = noopWrite,
    delete_file: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void = noopDelete,
    file_exists: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool = noopExists,
};

fn noopRead(ptr: *anyopaque, allocator: std.mem.Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
    _ = ptr;
    _ = allocator;
    _ = dir_path;
    _ = file_name;
    _ = max;
    return null;
}

fn noopWrite(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
    _ = data;
    return false;
}

fn noopDelete(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
}

fn noopExists(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
    return false;
}
