// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Shared build wiring for the SDL2-family backends.
//!
//! The three SDL2 presenters live in `src/backend/SDL2/render/<api>/render.zig`.
//! That deep module root cannot reach sibling files with a relative import, so
//! the shared pieces are published as their own modules under stable names:
//!
//!   - `neko_sdl2_platform` — `src/backend/SDL2/platform.zig` (window, events,
//!     timing, files) plus the SDL C bindings.
//!   - `neko_sdl2_render` — `src/backend/SDL2/render/render.zig` (common NDC
//!     maths).
//!
//! The presenters import those names directly.

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../plugin.zig");

/// The SDL2 libraries every desktop presenter links.
pub const SDL_LIBS: [4][]const u8 = .{ "sdl2", "SDL2_ttf", "SDL2_image", "SDL2_mixer" };

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

/// The shared SDL platform module (`src/backend/SDL2/platform.zig`).
pub fn platformModule(ctx: backend.Context, c_module: *Build.Module) *Build.Module {
    return ctx.b.addModule("neko_sdl2_platform", .{
        .root_source_file = ctx.b.path("src/backend/SDL2/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = c_module },
            .{ .name = "neko", .module = ctx.neko },
        },
    });
}

/// The common render helpers module (`src/backend/SDL2/render/render.zig`).
pub fn renderModule(ctx: backend.Context) *Build.Module {
    return ctx.b.addModule("neko_sdl2_render", .{
        .root_source_file = ctx.b.path("src/backend/SDL2/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "neko", .module = ctx.neko }},
    });
}
