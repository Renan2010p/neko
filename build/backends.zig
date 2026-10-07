// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The backend registry. Add new backends here.

const std: type = @import("std");
const backend: type = @import("backend.zig");

const sdl2: type = @import("backends/sdl2.zig");
const sdl2_opengl: type = @import("backends/sdl2_opengl.zig");
const sdl2_vulkan: type = @import("backends/sdl2_vulkan.zig");
const ps2: type = @import("backends/ps2.zig");

/// Every backend this package can build. The first entry is the default.
pub const all: []const backend.Backend = &.{
    sdl2.plugin,
    sdl2_opengl.plugin,
    sdl2_vulkan.plugin,
    ps2.plugin,
};

/// Looks a backend up by its `-Dbackend=<name>` value.
pub fn find(name: []const u8) ?backend.Backend {
    for (all) |be| {
        if (std.mem.eql(u8, be.name, name)) return be;
    }
    return null;
}

/// A comma-separated list of the available backend names (for error messages).
pub const names: []const u8 = blk: {
    var s: []const u8 = "";
    for (all, 0..) |be, i| {
        if (i != 0) s = s ++ ", ";
        s = s ++ be.name;
    }
    break :blk s;
};
