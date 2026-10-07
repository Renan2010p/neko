// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `save.Writer` — builds a little-endian byte buffer.

const std: type = @import("std");
const mem: type = std.mem;
const Allocator: type = std.mem.Allocator;
const Bytes: type = std.ArrayListUnmanaged(u8);

/// Builds a byte buffer.
pub const Writer: type = struct {
    allocator: Allocator,
    buf: Bytes = .empty,

    pub fn init(allocator: Allocator) Writer {
        return Writer{ .allocator = allocator };
    }

    pub fn deinit(self: *Writer) void {
        self.buf.deinit(self.allocator);
    }

    pub fn slice(self: *const Writer) []const u8 {
        return self.buf.items;
    }

    pub fn raw(self: *Writer, b: []const u8) !void {
        try self.buf.appendSlice(self.allocator, b);
    }

    pub fn byte(self: *Writer, v: u8) !void {
        try self.buf.append(self.allocator, v);
    }

    pub fn flag(self: *Writer, v: bool) !void {
        try self.byte(if (v) 1 else 0);
    }

    pub fn int(self: *Writer, v: i32) !void {
        var b: [4]u8 = undefined;
        mem.writeInt(i32, &b, v, .little);
        try self.raw(&b);
    }

    pub fn uint(self: *Writer, v: u32) !void {
        var b: [4]u8 = undefined;
        mem.writeInt(u32, &b, v, .little);
        try self.raw(&b);
    }

    pub fn size(self: *Writer, v: u64) !void {
        var b: [8]u8 = undefined;
        mem.writeInt(u64, &b, v, .little);
        try self.raw(&b);
    }

    pub fn real(self: *Writer, v: f32) !void {
        var b: [4]u8 = undefined;
        mem.writeInt(u32, &b, @bitCast(v), .little);
        try self.raw(&b);
    }

    /// Length (u32) + bytes — the C++ snapshot string layout.
    pub fn str_u32(self: *Writer, s: []const u8) !void {
        try self.uint(@intCast(s.len));
        try self.raw(s);
    }

    /// Length (u64) + bytes — the C++ `size_t` string layout.
    pub fn str_u64(self: *Writer, s: []const u8) !void {
        try self.size(@intCast(s.len));
        try self.raw(s);
    }
};
