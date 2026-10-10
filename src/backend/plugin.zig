// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Build-time backend plugin contract.
//!
//! A backend is a self-contained plugin that publishes a Zig module under the
//! stable import name `neko_backend`. The engine (`src/core/platform.zig`) reaches
//! it only through that name, so switching backends changes nothing in
//! `src/core/**`.
//!
//! To add a backend:
//!   1. Implement `src/backend/<Name>/render/render.zig`, exporting:
//!        - `pub const kind: neko.BackendKind = .{ .name = "<name>" };`
//!        - `pub fn create() neko.Backend`
//!   2. Add a `build.zig` next to the renderer with a `plugin: Backend`
//!      (see below), importing this contract.
//!   3. Register it in `registry.zig`.
//!
//! Nothing else needs to change.

const std: type = @import("std");
const Build: type = std.Build;

/// What a backend needs in order to wire itself into the build.
pub const Context: type = struct {
    b: *Build,
    /// The engine module (the backend imports it as `neko`).
    neko: *Build.Module,
    target: Build.ResolvedTarget,
    optimize: std.lang.Optimize,
    /// When true (the default) the backend modules are published under their
    /// stable names, so a dependent package can fetch one with
    /// `dependency.module("neko_backend")`. `zig build check-targets` compiles
    /// the same backend many times and sets this to false to build private
    /// modules instead: Zig 0.17 rejects two modules with the same name.
    publish: bool = true,
    /// Optional system-library directories. Only the SDL2 backend uses them
    /// (Windows has no pkg-config); other backends can ignore them.
    sdl2_include: ?[]const u8 = null,
    sdl2_lib: ?[]const u8 = null,
};

/// Creates a backend module, published (findable by name) or private according
/// to `Context.publish`.
pub fn module(ctx: Context, name: []const u8, options: Build.Module.CreateOptions) *Build.Module {
    return if (ctx.publish) ctx.b.addModule(name, options) else ctx.b.createModule(options);
}

/// A backend plugin.
pub const Backend: type = struct {
    /// The value of `-Dbackend=<name>`.
    name: []const u8,
    /// One-line description shown in `zig build --help` / `zig build backends`.
    description: []const u8,
    /// True when the backend only makes sense on a hosted target.
    hosted_only: bool = true,
    /// Creates and returns the module published as `neko_backend`.
    build: *const fn (Context) *Build.Module,
    /// Links the platform libraries into a module (tests / extension). May be
    /// a no-op for backends whose linker handles it elsewhere (the PS2).
    link: *const fn (*Build.Module) void,
};
