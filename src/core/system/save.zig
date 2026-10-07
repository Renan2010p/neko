//! `neko.save` — little-endian binary serialization and save-file helpers.
//!
//! Build bytes with `Writer`, store the whole buffer with `write_file`, and
//! load it back with `read_file` + `Reader`. All multi-byte values are
//! little-endian, so saves are portable between machines.
//!
//! File access goes through the backend, never the core: on a platform without
//! a filesystem (e.g. the PS2) `write_file`/`read_file` quietly degrade to
//! "no save file". `Writer`/`Reader` themselves are pure and always work.

const std: type = @import("std");
const mem: type = std.mem;
const Allocator: type = std.mem.Allocator;
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

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

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "save: Writer/Reader round-trips every field type" {
    var w: Writer = Writer.init(testing.allocator);
    defer w.deinit();

    try w.byte(7);
    try w.flag(true);
    try w.int(-1234);
    try w.uint(4000000000);
    try w.size(0x1122334455667788);
    try w.real(1.5);
    try w.str_u32("neko");
    try w.str_u64("engine");

    var r: Reader = Reader.init(w.slice());
    try testing.expectEqual(@as(u8, 7), try r.byte());
    try testing.expectEqual(true, try r.flag());
    try testing.expectEqual(@as(i32, -1234), try r.int());
    try testing.expectEqual(@as(u32, 4000000000), try r.uint());
    try testing.expectEqual(@as(u64, 0x1122334455667788), try r.size());
    try testing.expectEqual(@as(f32, 1.5), try r.real());
    try testing.expectEqualStrings("neko", try r.str_u32());
    try testing.expectEqualStrings("engine", try r.str_u64());
    try testing.expectEqual(@as(usize, 0), r.remaining());
}

test "save: Reader rejects a truncated buffer" {
    var w: Writer = Writer.init(testing.allocator);
    defer w.deinit();
    try w.uint(1);

    var r: Reader = Reader.init(w.slice());
    try testing.expectError(error.EndOfStream, r.size());
}
