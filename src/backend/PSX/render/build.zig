// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 1 backend plugin (pure Zig, freestanding, no SDK).
//!
//! The game cross-compiles directly to `mipsel-freestanding` (Zig's own MIPS-I
//! codegen), so nothing is linked here. The game's build must, when
//! `backend == .psx`:
//!
//!   * use `src/backend/PSX/entry.zig` as the executable root, importing the
//!     game module as `game` and the backend module as `neko_backend`;
//!   * call `exe.setLinkerScript(neko_dep.path("src/backend/PSX/platform/linker.ld"))`,
//!     set `exe.entry = .{ .symbol_name = "_start" }` and
//!     `exe.bundle_compiler_rt = false` (compiler-rt has atomics the R3000A
//!     cannot encode).

const std: type = @import("std");
const Build: type = std.Build;
const backend: type = @import("../../plugin.zig");

pub const plugin: backend.Backend = .{
    .name = "psx",
    .description = "PlayStation 1 (pure Zig, freestanding)",
    .hosted_only = false,
    .build = build,
    .link = link,
};

fn build(ctx: backend.Context) *Build.Module {
    const platform: *Build.Module = ctx.b.addModule("neko_psx_platform", .{
        .root_source_file = ctx.b.path("src/backend/PSX/platform.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
    });
    return ctx.b.addModule("neko_backend", .{
        .root_source_file = ctx.b.path("src/backend/PSX/render/render.zig"),
        .target = ctx.target,
        .optimize = ctx.optimize,
        .imports = &.{
            .{ .name = "neko", .module = ctx.neko },
            .{ .name = "neko_psx_platform", .module = platform },
        },
    });
}

fn link(_: *Build.Module) void {}
