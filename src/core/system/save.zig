//! `neko.save` — little-endian binary serialization and save-file helpers.
//!
//! Build bytes with `Writer`, store the whole buffer with `write_file`, and
//! load it back with `read_file` + `Reader`. All multi-byte values are
//! little-endian, so saves are portable between machines.
//!
//! File access goes through the backend, never the core: on a platform without
//! a filesystem (e.g. the PS2) `write_file`/`read_file` quietly degrade to
//! "no save file". `Writer`/`Reader` themselves are pure and always work.
//!
//! This is the public facade; the implementation lives one file per concern
//! under `save/`.

const files: type = @import("save/files.zig");

pub const Writer: type = @import("save/writer.zig").Writer;
pub const Reader: type = @import("save/reader.zig").Reader;

pub const write_file: *const fn ([]const u8, []const u8, []const u8) bool = files.write_file;
pub const read_file: *const fn (std.mem.Allocator, []const u8, []const u8, usize) ?[]u8 = files.read_file;
pub const delete_file: *const fn ([]const u8, []const u8) void = files.delete_file;
pub const file_exists: *const fn ([]const u8, []const u8) bool = files.file_exists;

const std: type = @import("std");

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
