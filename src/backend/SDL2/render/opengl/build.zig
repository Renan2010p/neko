// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! SDL2 + OpenGL backend plugin (an SDL2 window with an OpenGL presenter).

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../../../plugin.zig");
const sdl2_common: type = @import("../../build_support.zig");

pub const plugin: backend.Backend = .{
    .name = "sdl2-opengl",
    .description = "SDL2 window + OpenGL presenter (pygame path)",
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    const b: *Build = ctx.b;

    const c_module: *Build.Module = sdl2_common.cModule(ctx, "src/backend/SDL2/platform/c/SDL2.h", &.{});
    const gl_module: *Build.Module = sdl2_common.cModule(ctx, "src/backend/SDL2/platform/c/gl.h", &.{"GL"});
    const platform: *Build.Module = sdl2_common.platformModule(ctx, c_module);
    const render: *Build.Module = sdl2_common.renderModule(ctx);

    const mod: *Build.Module = b.addModule("neko_backend", .{
        .root_source_file = b.path("src/backend/SDL2/render/opengl/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = c_module },
            .{ .name = "gl", .module = gl_module },
            .{ .name = "neko", .module = ctx.neko },
            .{ .name = "neko_sdl2_platform", .module = platform },
            .{ .name = "neko_sdl2_render", .module = render },
        },
    });
    link(mod);
    if (ctx.sdl2_include) |inc| mod.addIncludePath(.{ .cwd_relative = inc });
    if (ctx.sdl2_lib) |lib| mod.addLibraryPath(.{ .cwd_relative = lib });
    return mod;
}

pub fn link(module: *Build.Module) void {
    for (sdl2_common.SDL_LIBS) |name| module.linkSystemLibrary(name, .{});
    module.linkSystemLibrary("GL", .{});
}
