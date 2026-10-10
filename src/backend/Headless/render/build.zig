// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Headless backend plugin (no OS, no window, no libc required).

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../../plugin.zig");

pub const plugin: backend.Backend = .{
    .name = "headless",
    .description = "No OS, no window: the engine loop only (tests, servers, bare metal)",
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    const platform: *Build.Module = backend.module(ctx, "neko_headless_platform", .{
        .root_source_file = ctx.b.path("src/backend/Headless/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
    });
    return backend.module(ctx, "neko_backend", .{
        .root_source_file = ctx.b.path("src/backend/Headless/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{
            .{ .name = "neko", .module = ctx.neko },
            .{ .name = "neko_headless_platform", .module = platform },
        },
    });
}

fn link(_: *Build.Module) void {}
