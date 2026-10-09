// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `check-freestanding` — fails if `src/core/**` references an OS API.
//!
//! The engine core must run without an operating system (the PS2/PSX backends
//! prove it). This tool walks the core tree and reports any real OS dependency
//! (`std.fs`, `std.net`, threads, POSIX, C bindings, raw `std.Io.Dir`/`File`).
//! Line comments are ignored, so documentation may name them.
//!
//! Usage: `check-freestanding <dir>` (e.g. `check-freestanding src/core`).

const std: type = @import("std");

/// Substrings that mark an OS dependency. Checked only in code (comments
/// stripped).
const forbidden: []const []const u8 = &.{
    "std.fs",
    "std.net",
    "std.Thread",
    "std.posix",
    "std.os",
    "std.c.",
    "std.Io.Dir",
    "std.Io.File",
    "std.Io.net",
    "@cImport",
};

fn stripComment(line: []const u8) []const u8 {
    const idx: ?usize = std.mem.indexOf(u8, line, "//");
    return if (idx) |i| line[0..i] else line;
}

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const gpa = init.gpa;

    var args = try init.minimal.args.iterateAllocator(gpa);
    defer args.deinit();
    _ = args.next(); // program name
    const root: []const u8 = args.next() orelse "src/core";

    const dir = try std.Io.Dir.cwd().openDir(io, root, .{ .iterate = true });
    defer dir.close(io);

    var walker = try dir.walk(gpa);
    defer walker.deinit();

    var violations: usize = 0;
    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.path, ".zig")) continue;

        const data = try entry.dir.readFileAlloc(io, entry.basename, gpa, .limited(1 << 20));
        defer gpa.free(data);

        var lines = std.mem.splitScalar(u8, data, '\n');
        var line_no: usize = 0;
        while (lines.next()) |line| {
            line_no += 1;
            const code: []const u8 = stripComment(line);
            for (forbidden) |needle| {
                if (std.mem.indexOf(u8, code, needle)) |_| {
                    std.debug.print("{s}:{d}: {s} (found `{s}`)\n", .{ entry.path, line_no, std.mem.trim(u8, line, " \t\r"), needle });
                    violations += 1;
                }
            }
        }
    }

    if (violations != 0) {
        std.debug.print("check-freestanding: {d} OS dependency(ies) in {s}\n", .{ violations, root });
        std.process.exit(1);
    }
    std.debug.print("check-freestanding: {s} is OS-free\n", .{root});
}
