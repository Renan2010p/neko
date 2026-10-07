// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! SDL2 backend plugin (desktop).

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../backend.zig");

const LIBS: [4][]const u8 = .{ "sdl2", "SDL2_ttf", "SDL2_image", "SDL2_mixer" };

pub const plugin: backend.Backend = .{
    .name = "sdl2",
    .description = "SDL2 + SDL2_ttf/image/mixer (desktop)",
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    const b: *Build = ctx.b;

    const c_translate: *Build.Step.TranslateC = b.addTranslateC(.{
        .root_source_file = b.path("src/backends/sdl2/SDL2.h"),
        .optimize = ctx.optimize,
        .target = ctx.target,
        .link_libc = true,
    });
    if (ctx.sdl2_include) |inc| c_translate.addIncludePath(.{ .cwd_relative = inc });
    for (LIBS) |name| c_translate.linkSystemLibrary(name, .{});

    const c_module: *Build.Module = c_translate.createModule();

    const mod: *Build.Module = b.addModule("neko_backend", .{
        .root_source_file = b.path("src/backends/sdl2/sdl2.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = c_module },
            .{ .name = "neko", .module = ctx.neko },
        },
    });
    link(mod);
    if (ctx.sdl2_include) |inc| mod.addIncludePath(.{ .cwd_relative = inc });
    if (ctx.sdl2_lib) |lib| mod.addLibraryPath(.{ .cwd_relative = lib });
    return mod;
}

pub fn link(module: *Build.Module) void {
    for (LIBS) |name| module.linkSystemLibrary(name, .{});
}
