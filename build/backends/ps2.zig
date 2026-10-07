// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 2 backend plugin (gsKit + pad, freestanding).
//!
//! Nothing is linked here: the target is freestanding, so the game is compiled
//! with Zig's C backend (`-ofmt=c`) and the emitted C is linked by the PS2DEV
//! toolchain against gsKit and the PS2SDK. This plugin only publishes the
//! module so `neko.screen` finds the backend.

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../backend.zig");

pub const plugin: backend.Backend = .{
    .name = "ps2",
    .description = "PlayStation 2 (gsKit + pad, freestanding)",
    .hosted_only = false,
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    return ctx.b.addModule("neko_backend", .{
        .root_source_file = ctx.b.path("src/platform/ps2/ps2.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{.{ .name = "neko", .module = ctx.neko }},
    });
}

fn link(_: *Build.Module) void {}
