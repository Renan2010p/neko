// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `save` file helpers. File access goes through the backend, never the core:
//! on a platform without a filesystem (e.g. the PS2) these quietly degrade to
//! "no save file".

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const context: type = @import("../../base/context.zig");
const backend: type = @import("../../base/backend.zig");

/// Writes `data` to `<dir_path>/<file_name>`, creating the directory.
/// Returns false when the backend has no filesystem or the write failed.
pub fn write_file(dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
    const e: backend.Backend = context.get() orelse return false;
    return e.write_file(dir_path, file_name, data);
}

/// Reads `<dir_path>/<file_name>` into a fresh buffer, or null if missing.
pub fn read_file(allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
    const e: backend.Backend = context.get() orelse return null;
    return e.read_file(allocator, dir_path, file_name, max);
}

/// Deletes `<dir_path>/<file_name>` if it exists.
pub fn delete_file(dir_path: []const u8, file_name: []const u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.delete_file(dir_path, file_name);
}

/// True if `<dir_path>/<file_name>` exists.
pub fn file_exists(dir_path: []const u8, file_name: []const u8) bool {
    const e: backend.Backend = context.get() orelse return false;
    return e.file_exists(dir_path, file_name);
}
