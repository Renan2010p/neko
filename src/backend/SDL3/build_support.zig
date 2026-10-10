// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Shared build wiring for the SDL3-family backends.
//!
//! The SDL3 presenters live in `src/backend/SDL3/render/<api>/render.zig`. The
//! shared pieces are published as modules under stable names:
//!
//!   - `neko_sdl3_platform` — `src/backend/SDL3/platform.zig`.
//!   - `neko_sdl3_render` — `src/backend/SDL3/render/render.zig`.

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../plugin.zig");

/// The SDL3 libraries every desktop presenter links.
pub const SDL_LIBS: [4][]const u8 = .{ "SDL3", "SDL3_ttf", "SDL3_image", "SDL3_mixer" };

/// Creates a `translate-c` module for `header` (plus `extra_libs`).
pub fn cModule(ctx: backend.Context, header: []const u8, extra_libs: []const []const u8) *Build.Module {
    const b: *Build = ctx.b;
    const translate: *Build.Step.TranslateC = b.addTranslateC(.{
        .root_source_file = b.path(header),
        .optimize = ctx.optimize,
        .target = ctx.target,
        .link_libc = true,
    });
    if (ctx.sdl2_include) |inc| translate.addIncludePath(.{ .cwd_relative = inc });
    for (SDL_LIBS) |name| translate.linkSystemLibrary(name, .{});
    for (extra_libs) |name| translate.linkSystemLibrary(name, .{});
    return translate.createModule();
}

/// The shared SDL platform module (`src/backend/SDL3/platform.zig`).
pub fn platformModule(ctx: backend.Context, c_module: *Build.Module) *Build.Module {
    return backend.module(ctx, "neko_sdl3_platform", .{
        .root_source_file = ctx.b.path("src/backend/SDL3/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = c_module },
            .{ .name = "neko", .module = ctx.neko },
        },
    });
}

/// The common render helpers module (`src/backend/SDL3/render/render.zig`).
pub fn renderModule(ctx: backend.Context) *Build.Module {
    return backend.module(ctx, "neko_sdl3_render", .{
        .root_source_file = ctx.b.path("src/backend/SDL3/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "neko", .module = ctx.neko }},
    });
}
