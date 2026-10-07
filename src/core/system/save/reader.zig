// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `save.Reader` — reads a little-endian byte buffer.

const std: type = @import("std");
const mem: type = std.mem;

/// Reads a byte buffer.
pub const Reader: type = struct {
    data: []const u8,
    pos: usize = 0,

    pub fn init(data: []const u8) Reader {
        return Reader{ .data = data };
    }

    fn take(self: *Reader, n: usize) ![]const u8 {
        if (self.pos + n > self.data.len) return error.EndOfStream;
        const s: []const u8 = self.data[self.pos .. self.pos + n];
        self.pos += n;
        return s;
    }

    /// Reads `n` raw bytes as a slice into the underlying data.
    pub fn raw(self: *Reader, n: usize) ![]const u8 {
        return self.take(n);
    }

    /// Bytes not yet consumed.
    pub fn remaining(self: *const Reader) usize {
        return self.data.len - self.pos;
    }

    pub fn byte(self: *Reader) !u8 {
        const s: []const u8 = try self.take(1);
        return s[0];
    }

    pub fn flag(self: *Reader) !bool {
        return (try self.byte()) != 0;
    }

    pub fn int(self: *Reader) !i32 {
        var b: [4]u8 = undefined;
        @memcpy(&b, try self.take(4));
        return mem.readInt(i32, &b, .little);
    }

    pub fn uint(self: *Reader) !u32 {
        var b: [4]u8 = undefined;
        @memcpy(&b, try self.take(4));
        return mem.readInt(u32, &b, .little);
    }

    pub fn size(self: *Reader) !u64 {
        var b: [8]u8 = undefined;
        @memcpy(&b, try self.take(8));
        return mem.readInt(u64, &b, .little);
    }

    pub fn real(self: *Reader) !f32 {
        var b: [4]u8 = undefined;
        @memcpy(&b, try self.take(4));
        return @bitCast(mem.readInt(u32, &b, .little));
    }

    pub fn str_u32(self: *Reader) ![]const u8 {
        const n: u32 = try self.uint();
        return self.take(n);
    }

    pub fn str_u64(self: *Reader) ![]const u8 {
        const n: u64 = try self.size();
        return self.take(@intCast(n));
    }
};
