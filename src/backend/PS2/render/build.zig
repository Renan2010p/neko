// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 2 backend plugin (gsKit + pad, freestanding).
//!
//! Nothing is linked here: the target is freestanding, so the game is compiled
//! with Zig's C backend (`-ofmt=c`) and the emitted C is linked by the PS2DEV
//! toolchain against gsKit and the PS2SDK. This plugin only publishes the
//! modules so `neko.screen` finds the backend.
//!
//! Two modules are published:
//!   - `neko_ps2_platform` — `src/backend/PS2/platform.zig` (PS2SDK + gsKit).
//!   - `neko_backend` — `src/backend/PS2/render/render.zig` (the renderer).

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../../plugin.zig");

pub const plugin: backend.Backend = .{
    .name = "ps2",
    .description = "PlayStation 2 (gsKit + pad, freestanding)",
    .hosted_only = false,
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    const platform: *Build.Module = backend.module(ctx, "neko_ps2_platform", .{
        .root_source_file = ctx.b.path("src/backend/PS2/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
    });
    return backend.module(ctx, "neko_backend", .{
        .root_source_file = ctx.b.path("src/backend/PS2/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{
            .{ .name = "neko", .module = ctx.neko },
            .{ .name = "neko_ps2_platform", .module = platform },
        },
    });
}

fn link(_: *Build.Module) void {}
