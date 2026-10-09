// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The backend registry. Add new backends here.

const std: type = @import("std");
const backend: type = @import("plugin.zig");

const sdl2: type = @import("SDL2/render/sdl/build.zig");
const sdl2_opengl: type = @import("SDL2/render/opengl/build.zig");
const sdl2_vulkan: type = @import("SDL2/render/vulkan/build.zig");
const ps2: type = @import("PS2/render/build.zig");
const psx: type = @import("PSX/render/build.zig");
const headless: type = @import("Headless/render/build.zig");

/// Every backend this package can build. The first entry is the default.
pub const all: []const backend.Backend = &.{
    sdl2.plugin,
    sdl2_opengl.plugin,
    sdl2_vulkan.plugin,
    ps2.plugin,
    psx.plugin,
    headless.plugin,
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
